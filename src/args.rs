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

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use argh::FromArgs;
use derive_more::FromStr;
use once_cell::sync::Lazy;

use crate::errors::SystemError;

#[derive(FromStr, Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct Endpoint(String);

impl Endpoint {
    pub(crate) fn new(path: &str) -> Endpoint {
        Endpoint(path.to_string())
    }

    pub(crate) fn standardize_to_socket(&self, stage: impl Display) -> String {
        let standard_path = self.0.clone().trim_matches('/').replace('/', "+");
        format!("ipc_{}_{}.socket", standard_path, stage)
    }

    pub(crate) fn path(&self) -> &str {
        &self.0
    }
}

static RESERVED_ROUTE: Lazy<HashSet<Endpoint>> =
    Lazy::new(|| HashSet::from([Endpoint::new("/"), Endpoint::new("/metrics")]));

#[derive(FromArgs, Debug, PartialEq)]
/// MOSEC arguments
pub(crate) struct Opts {
    /// the Unix domain socket directory path
    #[argh(option, default = "String::from(\"\")")]
    pub(crate) path: String,

    /// provided endpoints
    /// for example: ["/infer", "/infer/image", "/preprocess"]
    #[argh(option)]
    pub(crate) endpoints: Vec<Endpoint>,

    /// stage count for each endpoint, by same order as `endpoints`,
    /// len(stages) == len(endpoints),
    #[argh(option)]
    pub(crate) stages: Vec<u32>,

    /// max batch size for each stage
    /// flatten by same order as `endpoints`, sum(batches) == len(endpoints),
    #[argh(option)]
    pub(crate) batches: Vec<u32>,

    /// capacity for the channel
    /// (when the channel is full, the new requests will be dropped with 429 Too Many Requests)
    #[argh(option, short = 'c', default = "1024")]
    pub(crate) capacity: usize,

    /// timeout for one request (milliseconds)
    #[argh(option, short = 't', default = "3000")]
    pub(crate) timeout: u64,

    /// wait time for each batch (milliseconds), use `waits` instead [deprecated]
    #[argh(option, short = 'w', default = "10")]
    pub(crate) wait: u64,

    /// max wait time for each stage
    /// by same order as `batches`, len(waits) == len(batches),
    #[argh(option)]
    pub(crate) waits: Vec<u64>,

    /// service host
    #[argh(option, short = 'a', default = "String::from(\"0.0.0.0\")")]
    pub(crate) address: String,

    /// service port
    #[argh(option, short = 'p', default = "8000")]
    pub(crate) port: u16,

    /// metrics namespace
    #[argh(option, short = 'n', default = "String::from(\"mosec_service\")")]
    pub(crate) namespace: String,

    /// enable debug log
    #[argh(option, short = 'd', default = "false")]
    pub(crate) debug: bool,

    /// response mime type, by same order as `endpoints`,
    /// len(mime) == len(endpoints),
    #[argh(option)]
    pub(crate) mime: Vec<String>,
}

impl Opts {
    pub(crate) fn validate(&self) -> Result<(), SystemError> {
        for r in self.endpoints.iter() {
            if RESERVED_ROUTE.contains(r) {
                return Err(SystemError::ArgsValidationFailed {
                    msg: format!(
                        "endpoints and stages array should have same length, but {} != {}",
                        self.endpoints.len(),
                        self.stages.len()
                    ),
                });
            }
        }
        if self.endpoints.len() != self.stages.len() {
            return Err(SystemError::ArgsValidationFailed {
                msg: format!(
                    "endpoints and stages array should have same length, but {} != {}",
                    self.endpoints.len(),
                    self.stages.len()
                ),
            });
        }
        if self.endpoints.len() != self.mime.len() {
            return Err(SystemError::ArgsValidationFailed {
                msg: format!(
                    "endpoints and mime array should have same length, but {} != {}",
                    self.endpoints.len(),
                    self.mime.len()
                ),
            });
        }
        if let Ok(stage_count) = usize::try_from(self.stages.iter().sum::<u32>()) {
            if self.batches.len() != stage_count {
                return Err(SystemError::ArgsValidationFailed {
                    msg: format!(
                        "endpoints and stages array should have same length, but {} != {}",
                        self.endpoints.len(),
                        self.stages.len()
                    ),
                });
            }
        } else {
            return Err(SystemError::ArgsValidationFailed {
                msg: format!("stages array convert to u32 failed, {:#?}", self.stages,),
            });
        }
        Ok(())
    }

    pub(crate) fn devide_into_endpoints(&self) -> HashMap<Endpoint, (Vec<u32>, Vec<u64>)> {
        let mut endpoint_opts: HashMap<Endpoint, (Vec<u32>, Vec<u64>)> = HashMap::new();
        let batch_ind = 0;
        for (i, e) in self.endpoints.iter().enumerate() {
            let stages = self.stages[i] as usize;
            let sub_batches = self.batches[batch_ind..batch_ind + stages].to_vec();
            let sub_waits = self.waits[batch_ind..batch_ind + stages].to_vec();
            endpoint_opts.insert(e.clone(), (sub_batches, sub_waits));
        }
        endpoint_opts
    }
}
