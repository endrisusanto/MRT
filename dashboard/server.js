import express from 'express';
import { createServer } from 'http';
import { WebSocketServer, WebSocket } from 'ws';
import cors from 'cors';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const PORT = parseInt(process.env.PORT || '3010', 10);
const app = express();
const server = createServer(app);
const wss = new WebSocketServer({ server, path: '/ws' });

app.use(cors());
app.use(express.json());
app.use(express.static(path.join(__dirname, 'public')));

// In-memory node registry
// Map<node_id, { id, name, ip, port, devices: [], lastSeen: number }>
const nodes = new Map();

// Broadcast helper to connected WebSocket clients
function broadcast(payload) {
  const data = JSON.stringify(payload);
  wss.clients.forEach((client) => {
    if (client.readyState === WebSocket.OPEN) {
      client.send(data);
    }
  });
}

// Health check
app.get('/health', (req, res) => {
  res.json({
    status: 'ok',
    service: 'mrt-dashboard',
    port: PORT,
    timestamp: new Date().toISOString()
  });
});

// Receive heartbeat from desktop nodes
app.post('/api/nodes/heartbeat', (req, res) => {
  const { node_id, node_name, ip_address, port, devices, timestamp } = req.body;

  if (!node_id) {
    return res.status(400).json({ error: 'node_id is required' });
  }

  // Derive client IP if ip_address is localhost or empty
  const remoteIp = req.headers['x-forwarded-for'] || req.socket.remoteAddress;
  const resolvedIp = (!ip_address || ip_address === '127.0.0.1') ? (remoteIp?.replace('::ffff:', '') || '127.0.0.1') : ip_address;

  nodes.set(node_id, {
    id: node_id,
    name: node_name || node_id,
    ip: resolvedIp,
    port: port || 7890,
    devices: Array.isArray(devices) ? devices : [],
    lastSeen: Date.now(),
    timestamp: timestamp || new Date().toISOString()
  });

  // Notify connected web clients
  broadcast({
    event: 'node_updated',
    node_id,
    device_count: Array.isArray(devices) ? devices.length : 0
  });

  res.json({ status: 'ok', received: true });
});

// List all nodes with status
app.get('/api/nodes', (req, res) => {
  const now = Date.now();
  const list = [];

  for (const [id, node] of nodes.entries()) {
    const isOnline = now - node.lastSeen < 35000; // 35 seconds threshold
    list.push({
      ...node,
      status: isOnline ? 'ONLINE' : 'OFFLINE',
      secondsAgo: Math.round((now - node.lastSeen) / 1000)
    });
  }

  res.json(list);
});

// List aggregated devices across all active nodes
app.get('/api/devices', (req, res) => {
  const now = Date.now();
  const allDevices = [];

  for (const [id, node] of nodes.entries()) {
    const isOnline = now - node.lastSeen < 35000;
    if (isOnline && Array.isArray(node.devices)) {
      node.devices.forEach((dev) => {
        allDevices.push({
          ...dev,
          node_id: node.id,
          node_name: node.name,
          node_ip: node.ip,
          node_port: node.port
        });
      });
    }
  }

  res.json(allDevices);
});

// Proxy operation to target node
app.post('/api/operations', async (req, res) => {
  const { node_ip, node_port, serial, operation } = req.body;

  if (!node_ip || !serial || !operation) {
    return res.status(400).json({ error: 'node_ip, serial, and operation are required' });
  }

  const endpoint = operation.toLowerCase();
  const targetUrl = `http://${node_ip}:${node_port || 7890}/devices/${encodeURIComponent(serial)}/${endpoint}`;

  try {
    const response = await fetch(targetUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' }
    });

    const data = await response.json();
    broadcast({
      event: 'operation_proxied',
      serial,
      operation,
      status: 'INITIATED'
    });

    res.json(data);
  } catch (err) {
    res.status(502).json({
      error: 'Failed to contact node agent',
      message: err.message
    });
  }
});

// WebSocket connection handling
wss.on('connection', (ws) => {
  ws.send(JSON.stringify({
    event: 'welcome',
    service: 'mrt-dashboard',
    active_nodes: nodes.size
  }));
});

server.listen(PORT, '0.0.0.0', () => {
  console.log(`[MRT Dashboard] Central Server listening on http://0.0.0.0:${PORT}`);
});
