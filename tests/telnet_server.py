#!/usr/bin/env python3
"""
Simple Telnet test server that sends periodic messages.
Used for testing the wasmCloud Telnet provider.
Listens on a TCP port and sends test messages to connected clients.
"""

import asyncio
import json
import os
import sys
from datetime import datetime


async def handle_client(reader, writer):
    """Handle a connected Telnet client by sending periodic test messages."""
    addr = writer.get_extra_info('peername')
    client_id = id(writer)
    print(f"Client {client_id} connected from {addr}")

    message_count = 0
    try:
        # Send welcome banner
        welcome = "Welcome to the Telnet Test Server\r\n"
        writer.write(welcome.encode())
        await writer.drain()
        print(f"Sent welcome to client {client_id}")

        while True:
            message_count += 1

            # Send text message
            text_msg = json.dumps({
                "type": "test",
                "count": message_count,
                "timestamp": datetime.utcnow().isoformat(),
                "message": f"Test message #{message_count}"
            })

            line = text_msg + "\r\n"
            writer.write(line.encode())
            await writer.drain()
            print(f"Sent to client {client_id}: {text_msg}")

            # Wait before sending next message
            await asyncio.sleep(3)

    except (ConnectionResetError, BrokenPipeError, ConnectionAbortedError):
        print(f"Client {client_id} disconnected")
    except Exception as e:
        print(f"Error with client {client_id}: {e}")
    finally:
        writer.close()
        try:
            await writer.wait_closed()
        except Exception:
            pass


async def main():
    """Start the Telnet test server."""
    host = "127.0.0.1"
    port = int(os.environ.get("TELNET_PORT", "2323"))

    print(f"Starting Telnet test server on {host}:{port}")
    print("Press Ctrl+C to stop")
    print("-" * 50)

    server = await asyncio.start_server(handle_client, host, port)

    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nServer stopped")
