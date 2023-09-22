use encoding_rs::{CoderResult, SHIFT_JIS};
use iced::{
    futures::{
        channel::mpsc::{self, Sender},
        join, SinkExt, StreamExt,
    },
    subscription, Subscription,
};
use log::debug;
use std::{
    future::{pending, Future},
    process::Stdio,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    task,
};

use crate::config::Config;

pub fn connect(config: Config) -> Subscription<OutputEvent> {
    struct Connect;

    subscription::channel(
        std::any::TypeId::of::<Connect>(),
        config.channel_buf_size,
        |mut send_output| async move {
            let (send_input, mut recv_input) = mpsc::channel(config.channel_buf_size);
            send_output
                .send(OutputEvent::Connected(send_input))
                .await
                .unwrap();

            debug!("Connecting to shell...");
            let mut shell_process = tokio::process::Command::new(config.shell.clone())
                .args(config.shell_args.clone())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            debug!("Connected to shell.");

            let stdout = shell_process.stdout.take().unwrap();
            let stderr = shell_process.stderr.take().unwrap();
            let mut stdin = shell_process.stdin.take().unwrap();

            let stdout_future = decode_output(
                stdout,
                |text| async {
                    debug!("Read stdout: {text:?}");
                    let mut set_output = send_output.clone();
                    set_output.send(OutputEvent::Stdout(text)).await.unwrap();
                },
                config.clone(),
            );
            let stderr_future = decode_output(
                stderr,
                |text| async {
                    debug!("Read stderr: {text:?}");
                    let mut send_output = send_output.clone();
                    send_output.send(OutputEvent::Stdout(text)).await.unwrap();
                },
                config.clone(),
            );

            let stdin_handle = task::spawn(async move {
                debug!("Waiting for input messages...");
                loop {
                    match recv_input.next().await {
                        Some(InputEvent::Stdin(text)) => {
                            debug!("Write stdin: {text:?}");
                            stdin.write(text.as_bytes()).await.unwrap();
                        }
                        None => break,
                    }
                }
            });

            join!(stdout_future, stderr_future);
            stdin_handle.abort();
            send_output.send(OutputEvent::Disconnected).await.unwrap();

            pending::<()>().await;
            unreachable!();
        },
    )
}

async fn decode_output<T: AsyncRead, F: Future>(
    bytestream: T,
    mut cb: impl FnMut(String) -> F,
    config: Config,
) {
    let mut bytestream = Box::pin(bytestream);
    let mut decoder = SHIFT_JIS.new_decoder_without_bom_handling();

    let mut readbuf = vec![0u8; config.read_buf_size];
    let mut decodebuf = vec![
        0u8;
        decoder
            .max_utf8_buffer_length(config.read_buf_size)
            .unwrap()
    ];

    let mut last = false;
    while !last {
        let nbytes = bytestream.read(&mut readbuf).await.unwrap();
        last = nbytes == 0;
        debug!("Read {} bytes", nbytes);
        let (result, nwritten, nread, replaced) =
            decoder.decode_to_utf8(&readbuf[..nbytes], &mut decodebuf, last);
        debug!("Decoded result={result:?} nwritten={nwritten} nread={nread} replaced={replaced}");
        // Can't have OutputFull result since decode_buf_size was set sufficiently large
        assert!(result == CoderResult::InputEmpty);
        cb(String::from_utf8(decodebuf[..nwritten].into()).unwrap()).await;
    }
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
