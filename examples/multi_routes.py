# Copyright 2023 MOSEC Authors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Example: Sample structures for using multiply routes."""

from typing import Any, Dict, List

from mosec import Server, Worker


class Preprocess(Worker):
    """Sample Class."""

    def forward(self, data: Dict[str, int]) -> int:
        num = int(data.get("num", 10))
        return num


class Even(Worker):
    """Sample Class."""

    def forward(self, data: List[int]) -> List[bool]:
        res = [d % 2 == 0 for d in data]
        return res


class Next(Worker):
    """Sample Class."""

    def forward(self, data: int) -> Dict[str, int]:
        return {"ans": data + 1}


class Ping(Worker):
    """Sample Class."""

    def forward(self, _data: Any) -> str:
        return "ok"


if __name__ == "__main__":
    server = Server()
    route_even = server.route("/math/even")
    route_even.append_worker(Preprocess)
    route_even.append_worker(Even, max_batch_size=2)

    route_next = server.route("/math/next")
    route_next.append_worker(Preprocess)
    route_next.append_worker(Next)

    server.route("/ping").append_worker(Ping)
    server.run()
