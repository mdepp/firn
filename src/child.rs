use crate::config::Config;
use anyhow::Context;
use anyhow::{anyhow, Result};
use encoding_rs::{CoderResult, SHIFT_JIS};
use iced::futures::channel::mpsc::{Receiver, Sender};
use iced::futures::{SinkExt, StreamExt};
use iced::{futures::channel::mpsc, subscription, Subscription};
use log::{debug, error, info};
use std::future::pending;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::{join, select};
use tokio_util::sync::CancellationToken;

pub fn subscribe_to_pty(config: Config) -> Subscription<OutputEvent> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        config.channel_buf_size,
        async move |mut send_output| {
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
    let write_to_pty = async move || -> Result<()> {
        loop {
            select! {
                _ = cloned_token.cancelled() => break,
                message = receiver.next() => match message {
                    Some(InputEvent::Stdin(text)) => {
                        debug!("Receive {text} from stdin");
                        pty_writer.write_all(text.as_bytes()).await?;
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
    let read_from_pty = async move || -> Result<()> {
        let mut decoder = SHIFT_JIS.new_decoder_without_bom_handling();
        let mut readbuf = vec![0u8; config.read_buf_size];
        let mut decodebuf = vec![
            0u8;
            decoder
                .max_utf8_buffer_length(config.read_buf_size)
                .ok_or(anyhow!("Could not find decodebuf length"))?
        ];

        let mut last = false;
        while !last {
            select! {
                _ = cloned_token.cancelled() => break,
                    nbytes = pty_reader.read(&mut readbuf) => match nbytes {
                        Ok(nbytes) => {
                            debug!("Read {nbytes} bytes from pty");
                            last = nbytes == 0;
                            let (result, nwritten, _, _) =
                                decoder.decode_to_utf8(&readbuf[..nbytes], &mut decodebuf, last);
                            assert!(
                                result == CoderResult::InputEmpty,
                                "Can't have OutputFull result since decode_buf_size was set sufficiently large"
                            );
                            let text = String::from_utf8(decodebuf[..nwritten].into())?;
                            cloned_sender.send(OutputEvent::Stdout(text)).await?;
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

    let cleanup = async move || -> Result<()> {
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
    Stdin(String),
}

#[derive(Debug, Clone)]
pub enum OutputEvent {
    Connected(Sender<InputEvent>),
    Disconnected,
    Stdout(String),
    Stderr(String),
}
