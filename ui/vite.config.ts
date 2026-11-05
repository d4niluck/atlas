import { defineConfig, type Plugin } from 'vite';

// The development proxy only accepts requests from this loopback origin and
// adds a per-run secret that never enters the browser bundle.
const token = process.env.ATLAS_DEV_TOKEN;
const guard: Plugin = {
  name: 'atlas-local-api-guard',
  configureServer(server) {
    server.middlewares.use('/api', (req, res, next) => {
      const host = req.headers.host;
      const origin = req.headers.origin;
      if (!token || host !== '127.0.0.1:5173' || (origin && origin !== 'http://127.0.0.1:5173')) {
        res.statusCode = 403;
        res.end('Forbidden');
        return;
      }
      next();
    });
  },
};
export default defineConfig({
  plugins: [guard],
  server: {
    host: '127.0.0.1', port: 5173, strictPort: true,
    proxy: { '/api': { target: 'http://127.0.0.1:8765', configure: p => p.on('proxyReq', req => { if (token) req.setHeader('X-Atlas-Token', token); }) } },
  },
});
