#!/usr/bin/env python3
"""Microgrid Agent — Entry point."""

import argparse
import asyncio
import signal
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # type: ignore[no-redef]


async def run(config_path: str, simulate: bool = False):
    from src.agent import MicrogridAgent

    with open(config_path, "rb") as f:
        config = tomllib.load(f)

    agent = MicrogridAgent(config, simulate=simulate)
    await agent.start()

    loop = asyncio.get_event_loop()
    stop_event = asyncio.Event()

    def handle_signal(sig: int, _frame):
        print(f"\nReceived signal {sig}, shutting down...")
        stop_event.set()

    signal.signal(signal.SIGTERM, handle_signal)
    signal.signal(signal.SIGINT, handle_signal)
    signal.signal(signal.SIGUSR1, lambda *_: asyncio.ensure_future(agent.reload_knowledge()))

    try:
        await stop_event.wait()
    finally:
        await agent.stop()


def main():
    parser = argparse.ArgumentParser(description="Microgrid Agent")
    parser.add_argument("--config", default="config/site.toml", help="Path to site config")
    parser.add_argument("--simulate", action="store_true", help="Run with simulated devices")
    args = parser.parse_args()

    if not Path(args.config).exists():
        print(f"Config not found: {args.config}")
        print("Copy config/site.example.toml to config/site.toml and edit it.")
        sys.exit(1)

    asyncio.run(run(args.config, args.simulate))


if __name__ == "__main__":
    main()
