// Copyright 2022 MOSEC Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_channel::{bounded, Receiver, Sender};
use tokio::sync::Barrier;
use tracing::{error, info};

use crate::args::Endpoint;
use crate::protocol::communicate;
use crate::tasks::TaskManager;

#[derive(Debug)]
pub(crate) struct Coordinator {
    capacity: usize,
    path: String,
    batches: Vec<u32>,
    wait_time: Vec<Duration>,
    receiver: Receiver<u32>,
    sender: Sender<u32>,
}

impl Coordinator {
    pub(crate) fn init(
        batches: Vec<u32>,
        waits: Vec<u64>,
        path: String,
        capacity: usize,
        sender: Sender<u32>,
        receiver: Receiver<u32>,
    ) -> Self {
        let wait_time = waits.iter().map(|x| Duration::from_millis(*x)).collect();
        let path = if !path.is_empty() {
            path
        } else {
            // default IPC path
            std::env::temp_dir()
                .join(env!("CARGO_PKG_NAME"))
                .into_os_string()
                .into_string()
                .unwrap()
        };

        Self {
            capacity,
            path,
            batches,
            wait_time,
            receiver,
            sender,
        }
    }

    pub(crate) fn run(&self, endpoint: Endpoint) -> Arc<Barrier> {
        let barrier = Arc::new(Barrier::new(self.batches.len() + 1));
        let mut last_receiver = self.receiver.clone();
        let mut last_sender = self.sender.clone();
        let folder = Path::new(&self.path);
        if folder.is_dir() {
            info!(path=?folder, "socket path already exist, try to remove it");
            fs::remove_dir_all(folder).unwrap();
        }
        fs::create_dir(folder).unwrap();

        for i in 0..self.batches.len() {
            let (sender, receiver) = bounded::<u32>(self.capacity);
            let path = folder.join(endpoint.standardize_to_socket(i + 1));

            let batch_size = self.batches[i];
            let wait = self.wait_time[i];
            tokio::spawn(communicate(
                endpoint.clone(),
                path,
                batch_size as usize,
                wait,
                (i + 1).to_string(),
                last_receiver.clone(),
                sender.clone(),
                last_sender.clone(),
                barrier.clone(),
            ));
            last_receiver = receiver;
            last_sender = sender;
        }
        tokio::spawn(finish_task(last_receiver, endpoint));
        barrier
    }
}

async fn finish_task(receiver: Receiver<u32>, endpoint: Endpoint) {
    let task_manager = TaskManager::global();
    loop {
        match receiver.recv().await {
            Ok(id) => {
                task_manager.notify_task_done(id, &endpoint);
            }
            Err(err) => {
                error!(%err, "failed to get the task id when trying to mark it as done");
            }
        }
    }
}
