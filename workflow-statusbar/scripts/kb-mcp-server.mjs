#!/usr/bin/env node

const DEFAULT_BASE_URL = "http://127.0.0.1:8788";
const apiBaseUrl = (process.env.KB_API_BASE_URL || DEFAULT_BASE_URL).replace(/\/+$/, "");

const tools = [
  {
    name: "search_memory",
    description: "Search local knowledge memory by keyword. Read-only.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Search keyword or phrase." }
      },
      required: ["query"]
    }
  },
  {
    name: "get_prompt_template",
    description: "List prompt templates, optionally filtered by scene and status. Read-only.",
    inputSchema: {
      type: "object",
      properties: {
        scene: { type: "string", description: "Scene keyword used to rank matching templates." },
        status: { type: "string", description: "Template status, such as verified or reviewed." }
      }
    }
  },
  {
    name: "build_task_context",
    description: "Build a task context package from a requirement, task id, or natural-language input. Read-only.",
    inputSchema: {
      type: "object",
      properties: {
        input: { type: "string", description: "Task input, REQ-ID, TASK-ID, or natural language." },
        limit: { type: "number", description: "Maximum evidence count." }
      },
      required: ["input"]
    }
  },
  {
    name: "get_evidence_trace",
    description: "Get an evidence item and its trace by item id. Read-only.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Knowledge item id." }
      },
      required: ["id"]
    }
  },
  {
    name: "list_asset_health",
    description: "List knowledge asset health summary, assets, projects, and suggested actions. Read-only.",
    inputSchema: {
      type: "object",
      properties: {}
    }
  }
];

let nextServerId = 1;
let inputBuffer = "";

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function result(id, value) {
  send({ jsonrpc: "2.0", id, result: value });
}

function error(id, code, message, data = undefined) {
  const payload = { jsonrpc: "2.0", id, error: { code, message } };
  if (data !== undefined) {
    payload.error.data = data;
  }
  send(payload);
}

function asTextContent(data) {
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(data, null, 2)
      }
    ]
  };
}

function requireString(args, key) {
  const value = args?.[key];
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`missing_required_argument:${key}`);
  }
  return value.trim();
}

function optionalString(args, key) {
  const value = args?.[key];
  return typeof value === "string" && value.trim() !== "" ? value.trim() : undefined;
}

function optionalLimit(args) {
  const value = args?.limit;
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return Math.min(Math.floor(value), 50);
  }
  return 8;
}

function summarizeArgs(args) {
  const entries = Object.entries(args || {})
    .slice(0, 8)
    .map(([key, value]) => {
      const text = typeof value === "string" ? value : JSON.stringify(value);
      return `${key}=${String(text || "").slice(0, 80)}`;
    });
  return entries.join("&").slice(0, 300);
}

async function apiJson(path, options = {}, callMeta = {}) {
  const response = await fetch(`${apiBaseUrl}${path}`, {
    ...options,
    headers: {
      "content-type": "application/json",
      "x-kb-client": "workflow-knowledgebase-mcp",
      "x-kb-client-id": "workflow-knowledgebase-mcp",
      "x-kb-tool": callMeta.toolName || "unknown",
      "x-kb-params": callMeta.paramsSummary || "",
      ...(options.headers || {})
    }
  });
  const text = await response.text();
  let data = {};
  if (text.trim() !== "") {
    data = JSON.parse(text);
  }
  if (!response.ok) {
    throw new Error(`api_error:${response.status}:${data.error || text}`);
  }
  return data;
}

function rankTemplatesByScene(data, scene) {
  if (!scene) {
    return data;
  }
  const lowerScene = scene.toLowerCase();
  const templates = Array.isArray(data?.data?.templates) ? data.data.templates : [];
  return {
    ...data,
    data: {
      ...data.data,
      templates: templates
        .map((template) => {
          const haystack = [
            template.title,
            template.scene,
            template.category,
            template.content,
            template.notes,
            template.output_format
          ]
            .filter(Boolean)
            .join("\n")
            .toLowerCase();
          return {
            ...template,
            mcp_scene_match: haystack.includes(lowerScene)
          };
        })
        .sort((a, b) => Number(b.mcp_scene_match) - Number(a.mcp_scene_match))
    }
  };
}

async function callTool(name, args) {
  if (name === "search_memory") {
    const query = encodeURIComponent(requireString(args, "query"));
    return apiJson(`/api/v1/search?q=${query}`, {}, {
      toolName: name,
      paramsSummary: summarizeArgs(args)
    });
  }
  if (name === "get_prompt_template") {
    const status = optionalString(args, "status");
    const scene = optionalString(args, "scene");
    const query = status ? `?status=${encodeURIComponent(status)}` : "";
    return rankTemplatesByScene(
      await apiJson(`/api/v1/templates${query}`, {}, {
        toolName: name,
        paramsSummary: summarizeArgs(args)
      }),
      scene
    );
  }
  if (name === "build_task_context") {
    return apiJson("/api/v1/task-context", {
      method: "POST",
      body: JSON.stringify({
        input_text: requireString(args, "input"),
        limit: optionalLimit(args)
      })
    }, {
      toolName: name,
      paramsSummary: summarizeArgs(args)
    });
  }
  if (name === "get_evidence_trace") {
    const id = encodeURIComponent(requireString(args, "id"));
    return apiJson(`/api/v1/evidence/${id}`, {}, {
      toolName: name,
      paramsSummary: summarizeArgs(args)
    });
  }
  if (name === "list_asset_health") {
    return apiJson("/api/v1/health", {}, {
      toolName: name,
      paramsSummary: summarizeArgs(args)
    });
  }
  throw new Error(`unknown_tool:${name}`);
}

async function handleRequest(request) {
  const id = request.id ?? nextServerId++;
  try {
    if (request.method === "initialize") {
      result(id, {
        protocolVersion: "2024-11-05",
        capabilities: { tools: {} },
        serverInfo: {
          name: "workflow-knowledgebase-mcp",
          version: "0.1.0"
        }
      });
      return;
    }
    if (request.method === "tools/list") {
      result(id, { tools });
      return;
    }
    if (request.method === "tools/call") {
      const name = request.params?.name;
      const args = request.params?.arguments || {};
      result(id, asTextContent(await callTool(name, args)));
      return;
    }
    if (request.method === "ping") {
      result(id, {});
      return;
    }
    if (request.method === "notifications/initialized") {
      return;
    }
    error(id, -32601, `Method not found: ${request.method}`);
  } catch (err) {
    error(id, -32000, err instanceof Error ? err.message : String(err));
  }
}

process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  inputBuffer += chunk;
  const lines = inputBuffer.split(/\r?\n/);
  inputBuffer = lines.pop() || "";
  for (const line of lines) {
    if (line.trim() === "") {
      continue;
    }
    try {
      void handleRequest(JSON.parse(line));
    } catch (err) {
      error(null, -32700, err instanceof Error ? err.message : String(err));
    }
  }
});

process.stdin.on("end", () => {
  if (inputBuffer.trim() === "") {
    return;
  }
  try {
    void handleRequest(JSON.parse(inputBuffer));
  } catch (err) {
    error(null, -32700, err instanceof Error ? err.message : String(err));
  }
});
