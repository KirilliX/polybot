// Простой сервер: отдаёт dist/, проксирует /api/gamma и /api/clob
const http  = require('http');
const https = require('https');
const fs    = require('fs');
const path  = require('path');
const url   = require('url');

const PORT = 3000;
const DIST = path.join(__dirname, 'dist');

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js':   'application/javascript',
  '.css':  'text/css',
  '.svg':  'image/svg+xml',
  '.png':  'image/png',
  '.ico':  'image/x-icon',
  '.json': 'application/json',
};

function proxyTo(hostname, req, res) {
  const targetPath = req.url.replace(/^\/api\/(gamma|clob)/, '') || '/';
  const options = {
    hostname,
    path:    targetPath,
    method:  'GET',
    headers: { 'User-Agent': 'polybot-ui/1.0', 'Accept': 'application/json' },
  };
  const proxy = https.request(options, (upstream) => {
    res.writeHead(upstream.statusCode, {
      'Content-Type':                'application/json',
      'Access-Control-Allow-Origin': '*',
    });
    upstream.pipe(res);
  });
  proxy.on('error', (e) => {
    res.writeHead(502);
    res.end(JSON.stringify({ error: e.message }));
  });
  proxy.end();
}

http.createServer((req, res) => {
  // ── Прокси на Gamma API ────────────────────────────────────────────────────
  if (req.url.startsWith('/api/gamma')) {
    proxyTo('gamma-api.polymarket.com', req, res);
    return;
  }

  // ── Прокси на CLOB API (живые цены из стакана) ────────────────────────────
  if (req.url.startsWith('/api/clob')) {
    proxyTo('clob.polymarket.com', req, res);
    return;
  }

  // ── Статика из dist/ ──────────────────────────────────────────────────────
  let filePath = path.join(DIST, req.url === '/' ? 'index.html' : req.url);
  if (!fs.existsSync(filePath)) filePath = path.join(DIST, 'index.html'); // SPA fallback

  const ext  = path.extname(filePath);
  const mime = MIME[ext] || 'application/octet-stream';

  fs.readFile(filePath, (err, data) => {
    if (err) { res.writeHead(404); res.end('Not found'); return; }
    res.writeHead(200, { 'Content-Type': mime });
    res.end(data);
  });
}).listen(PORT, () => console.log(`polybot-ui listening on :${PORT}`));
