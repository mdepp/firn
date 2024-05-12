use crate::config::Config;
use anyhow::Context;
use anyhow::Result;
use iced::futures::channel::mpsc::{Receiver, Sender};
use iced::futures::{SinkExt, StreamExt};
use iced::{futures::channel::mpsc, subscription, Subscription};
use log::{debug, error, info};
use std::future::pending;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time;
use tokio::{join, select};
use tokio_util::sync::CancellationToken;

pub fn subscribe_to_pty(config: Config) -> Subscription<OutputEvent> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        config.channel_buf_size,
        async move |mut send_output: Sender<OutputEvent>| {
            let config = config.clone();
            let (send_input, recv_input) = mpsc::channel(config.channel_buf_size);
            send_output
                .send(OutputEvent::Connected(send_input))
                .await
                .unwrap();

            make_pty(config, send_output.clone(), recv_input)
                .await
                .with_context(|| "make_pty")
                .unwrap();

            send_output.send(OutputEvent::Disconnected).await.unwrap();

            pending::<()>().await;
            unreachable!();
        },
    )
}

async fn make_pty(
    config: Config,
    sender: Sender<OutputEvent>,
    mut receiver: Receiver<InputEvent>,
) -> Result<()> {
    let mut pty = pty_process::Pty::new()?;
    let mut cmd = pty_process::Command::new(config.shell)
        .args(config.shell_args)
        .spawn(&pty.pts()?)?;

    let (mut pty_reader, mut pty_writer) = pty.split();
    let cancellation_token = CancellationToken::new();

    let cloned_token = cancellation_token.clone();
    let mut write_to_pty = async move || -> Result<()> {
        loop {
            select! {
                _ = cloned_token.cancelled() => break,
                message = receiver.next() => match message {
                    Some(InputEvent::Stdin(text)) => {
                        debug!("Receive {text:?} from stdin");
                        pty_writer.write_all(&text).await?;
                        debug!("Sent to pty");
                    }
                    None => break
                }
            }
        }
        debug!("Shutting down pty writer");
        pty_writer.shutdown().await?;
        Ok(())
    };

    let mut cloned_sender = sender.clone();
    let cloned_token = cancellation_token.clone();
    let mut read_from_pty = async move || -> Result<()> {
        let mut readbuf = vec![0u8; config.read_buf_size];

        loop {
            select! {
                _ = cloned_token.cancelled() => break,
                    nbytes = pty_reader.read(&mut readbuf) => match nbytes {
                        Ok(0) => {
                            debug!("pty finished sending bytes");
                            break;
                        }
                        Ok(nbytes) => {
                            debug!("Read {nbytes} bytes from pty");
                            cloned_sender.send(OutputEvent::Stdout(readbuf[..nbytes].into())).await?;
                            // HACK: throttle pty output messages to avoid overwhelming iced
                            time::sleep(time::Duration::from_millis(10)).await;
                        }
                        Err(err) => {
                            error!("pty read error: {err}");
                            break;
                        }
                }
            }
        }
        debug!("Shutting down pty reader");
        Ok(())
    };

    let mut cleanup = async move || -> Result<()> {
        let status = cmd.wait().await?;
        info!("Shell finished with status {status}");
        cancellation_token.cancel();
        Ok(())
    };

    let result = join!(write_to_pty(), read_from_pty(), cleanup());
    result
        .0
        .with_context(|| "write_to_pty")
        .and(result.1.with_context(|| "read_from_pty"))
        .and(result.2.with_context(|| "cleanup"))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub enum InputEvent {
    Stdin(Vec<u8>),
}

#[derive(Debug, Clone)]
pub enum OutputEvent {
    Connected(Sender<InputEvent>),
    Disconnected,
    Stdout(Vec<u8>),
}
