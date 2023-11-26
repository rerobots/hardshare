// Copyright (C) 2023 rerobots, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::process::Command;


fn check_podman() -> Result<(), String> {
    let output = match Command::new("podman").arg("version").output() {
        Ok(x) => x,
        Err(err) => {
            return Err(err.to_string())
        }
    };
    Ok(())
}


pub fn defaults() -> Result<(), String> {
    Ok(())
}
