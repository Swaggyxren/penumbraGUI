/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2024 Shomy
*/

use log::warn;
use penumbra::DeviceLog;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

pub async fn setup_file_logger(path: &str) -> Option<DeviceLog> {
    match OpenOptions::new().create(true).append(true).open(path).await {
        Ok(mut file) => {
            let (tx, mut rx) = mpsc::channel::<String>(100);

            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    if file.write_all(msg.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });

            let device_log = DeviceLog::with_on_push(Box::new(move |msg| {
                let _ = tx.try_send(format!("{}\n", msg));
            }));

            Some(device_log)
        }
        Err(e) => {
            warn!("Failed to open {} for writing device logs: {}", path, e);
            None
        }
    }
}
