import fs from 'node:fs/promises';
import path from 'node:path';

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const STATE_FILES = ['.ai/runtime/project-state.json', 'project-state.json'];

async function readProjectState(projectPath) {
  const normalizedPath = path.resolve(projectPath);

  for (const relativePath of STATE_FILES) {
    const candidate = path.join(normalizedPath, relativePath);
    try {
      const raw = await fs.readFile(candidate, 'utf-8');
      const parsed = JSON.parse(raw);
      const stat = await fs.stat(candidate);
      return {
        found: true,
        projectPath: normalizedPath,
        statePath: candidate,
        state: parsed,
        updatedAt: stat.mtime.toISOString(),
      };
    } catch (error) {
      if (error?.code === 'ENOENT') {
        continue;
      }
      if (error instanceof SyntaxError) {
        return {
          found: false,
          projectPath: normalizedPath,
          error: `状态文件 JSON 解析失败: ${candidate}`,
        };
      }
      return {
        found: false,
        projectPath: normalizedPath,
        error: `读取状态文件失败: ${candidate}`,
      };
    }
  }

  return {
    found: false,
    projectPath: normalizedPath,
    missingFiles: STATE_FILES.map((relativePath) => path.join(normalizedPath, relativePath)),
  };
}

function cockpitBridgePlugin() {
  const handler = async (req, res) => {
    if (!req.url) {
      res.statusCode = 400;
      res.end(JSON.stringify({ error: '请求地址无效' }));
      return;
    }

    const url = new URL(req.url, 'http://localhost');
    if (url.pathname !== '/api/project-state') {
      return;
    }

    const projectPath = url.searchParams.get('path');
    res.setHeader('Content-Type', 'application/json; charset=utf-8');

    if (!projectPath) {
      res.statusCode = 400;
      res.end(JSON.stringify({ error: '缺少 path 参数' }));
      return;
    }

    const result = await readProjectState(projectPath);
    res.statusCode = result.error ? 500 : 200;
    res.end(JSON.stringify(result));
  };

  return {
    name: 'project-cockpit-bridge',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url?.startsWith('/api/project-state')) {
          handler(req, res).catch((error) => {
            res.statusCode = 500;
            res.setHeader('Content-Type', 'application/json; charset=utf-8');
            res.end(JSON.stringify({ error: error?.message || '服务异常' }));
          });
          return;
        }
        next();
      });
    },
    configurePreviewServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url?.startsWith('/api/project-state')) {
          handler(req, res).catch((error) => {
            res.statusCode = 500;
            res.setHeader('Content-Type', 'application/json; charset=utf-8');
            res.end(JSON.stringify({ error: error?.message || '服务异常' }));
          });
          return;
        }
        next();
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), cockpitBridgePlugin()],
});
