#!/usr/bin/env node
// Loopback TCP -> instance Unix socket proxy, so a browser can load the web UI of the
// isolated agent-test instance (its TCP port needs Basic auth that nobody configured).
// Usage: node socket-proxy.mjs [port=19890]   (stop with Ctrl-C)
import http from "node:http";
import { SOCKET } from "./lib.mjs";

const port = Number(process.argv[2] ?? 19890);
http.createServer((req, res) => {
  const up = http.request({ socketPath: SOCKET, method: req.method, path: req.url, headers: { ...req.headers, host: "localhost" } }, (r) => {
    res.writeHead(r.statusCode, r.headers);
    r.pipe(res);
  });
  up.on("error", (e) => { res.writeHead(502); res.end(String(e)); });
  req.pipe(up);
}).listen(port, "127.0.0.1", () => console.log(`proxy http://127.0.0.1:${port} -> ${SOCKET}`));
