export class LoroWebsocketServer {
  started = false;

  start(port: number): string {
    // Placeholder implementation for basic setup
    this.started = true;
    return `listening:${port}`;
  }
}

export { SimpleServer, type SimpleServerConfig } from "./simple-server";
