"""Local-only HTTP companion for the SteadyUI browser extension.

Run with ``python -m steadyui_ml.server``. The server deliberately binds to
127.0.0.1, never to the local network.
"""

from __future__ import annotations

import argparse
import json
import secrets
import sys
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

from .ingestion import receive_calibration_trial, receive_interaction

ML_DIRECTORY = Path(__file__).resolve().parents[2]
DEFAULT_RAW_DIRECTORY = ML_DIRECTORY / "data" / "raw"
DEFAULT_MODEL_PATH = ML_DIRECTORY / "models" / "active-model.joblib"
MAX_BODY_BYTES = 1_000_000


class CompanionHandler(BaseHTTPRequestHandler):
    token = ""
    raw_directory = DEFAULT_RAW_DIRECTORY
    model_path = DEFAULT_MODEL_PATH

    def log_message(self, format: str, *args: object) -> None:
        """Avoid logging payload contents, which may contain behavioural data."""
        return

    def _reply(self, status: HTTPStatus, payload: dict[str, Any]) -> None:
        encoded = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, X-SteadyUI-Token")
        self.end_headers()
        self.wfile.write(encoded)

    def _authorized(self) -> bool:
        return secrets.compare_digest(self.headers.get("X-SteadyUI-Token", ""), self.token)

    def _payload(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        if not 0 < length <= MAX_BODY_BYTES:
            raise ValueError(f"request body must be between 1 and {MAX_BODY_BYTES} bytes")
        payload = json.loads(self.rfile.read(length))
        if not isinstance(payload, dict):
            raise ValueError("request body must be a JSON object")
        return payload

    def do_OPTIONS(self) -> None:  # noqa: N802
        self.send_response(HTTPStatus.NO_CONTENT)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
        self.send_header("Access-Control-Allow-Headers", "Content-Type, X-SteadyUI-Token")
        self.end_headers()

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/health":
            self._reply(HTTPStatus.OK, {"status": "ok"})
        else:
            self._reply(HTTPStatus.NOT_FOUND, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        if not self._authorized():
            self._reply(HTTPStatus.UNAUTHORIZED, {"error": "invalid companion token"})
            return
        try:
            payload = self._payload()
            if self.path == "/calibration-trials":
                trial = receive_calibration_trial(payload, self.raw_directory / "calibration.jsonl")
                response = {"status": "stored", "trial_id": trial.trial_id}
            elif self.path == "/interactions":
                record = receive_interaction(payload, self.raw_directory / "trajectories.jsonl")
                response = {"status": "stored", "interaction_id": record.interaction_id}
            elif self.path == "/predict":
                from .prediction import predict

                response = predict(payload, self.model_path)
            elif self.path == "/train":
                from .training import train_model

                result = train_model(
                    self.raw_directory / "trajectories.jsonl", self.model_path
                )
                response = {"status": "trained", "metrics": result.__dict__}
            else:
                self._reply(HTTPStatus.NOT_FOUND, {"error": "not found"})
                return
        except (FileNotFoundError, ValueError, json.JSONDecodeError) as error:
            self._reply(HTTPStatus.BAD_REQUEST, {"error": str(error)})
            return
        self._reply(HTTPStatus.OK, response)


def main(argv: list[str] | None = None) -> None:
    # ``argparse`` treats a value beginning with ``-`` as a new option, even if
    # it follows ``--token``. URL-safe random tokens can legitimately begin
    # with a dash, so normalize this pair to argparse's unambiguous equals form.
    command_line = list(sys.argv[1:] if argv is None else argv)
    for index, argument in enumerate(command_line[:-1]):
        if argument == "--token":
            command_line[index : index + 2] = [f"--token={command_line[index + 1]}"]
            break

    parser = argparse.ArgumentParser(description="Run the local SteadyUI companion API.")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--raw-directory", type=Path, default=DEFAULT_RAW_DIRECTORY)
    parser.add_argument("--model-path", type=Path, default=DEFAULT_MODEL_PATH)
    parser.add_argument("--token", required=True, help="Token configured in the extension options page")
    arguments = parser.parse_args(command_line)
    if not 1 <= arguments.port <= 65535:
        parser.error("port must be between 1 and 65535")

    CompanionHandler.token = arguments.token
    CompanionHandler.raw_directory = arguments.raw_directory.resolve()
    CompanionHandler.model_path = arguments.model_path.resolve()
    server = ThreadingHTTPServer(("127.0.0.1", arguments.port), CompanionHandler)
    print(f"SteadyUI companion listening on http://127.0.0.1:{arguments.port}")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
