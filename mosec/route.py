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

"""MOSEC routes configurations."""

from contextlib import contextmanager
from typing import TYPE_CHECKING, Dict, List, Type, Union

from mosec.worker import Worker

if TYPE_CHECKING:
    from mosec.server import Server


class Route:
    """MOSEC Route for server.

    It allows users to attach pipelines to arbitrary routes.
    """

    # pylint: disable=too-few-public-methods

    def __init__(self, route_path: str, server: "Server"):
        """Initialize a MOSEC Route.

        Args:
            route_path: picked path of route
            server: MOSEC server instance to build route on
        """
        self.route_path = route_path
        self.server = server

    @contextmanager
    def _specify_route(self):
        resolved_path = self.server.route_path
        self.server.route_path = self.route_path
        try:
            yield None
        finally:
            self.server.route_path = resolved_path

    # pylint: disable=too-many-arguments
    def append_worker(
        self,
        worker: Type[Worker],
        num: int = 1,
        max_batch_size: int = 1,
        max_wait_time: int = 0,
        start_method: str = "spawn",
        env: Union[None, List[Dict[str, str]]] = None,
        timeout: int = 0,
    ):
        """Sequentially appends workers to the workflow pipeline of specify route.

        Args:
            worker: the class you inherit from :class:`Worker<mosec.worker.Worker>`
                which implements the :py:meth:`forward<mosec.worker.Worker.forward>`
            num: the number of processes for parallel computing (>=1)
            max_batch_size: the maximum batch size allowed (>=1), will enable the
                dynamic batching if it > 1
            max_wait_time: the maximum wait time (millisecond) for dynamic batching,
                needs to be used with `max_batch_size` to enable the feature. If not
                configure, will use the CLI argument `--wait` (default=10ms)
            start_method: the process starting method ("spawn" or "fork")
            env: the environment variables to set before starting the process
            timeout: the timeout (second) for each worker forward processing (>=1)
        """
        with self._specify_route():
            self.server.append_worker(
                worker, num, max_batch_size, max_wait_time, start_method, env, timeout
            )
        return self
