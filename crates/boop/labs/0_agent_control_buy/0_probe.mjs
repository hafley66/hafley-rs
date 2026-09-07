#!/usr/bin/env node
// Provider-free ACPX probe. It writes a minimal ACP agent into a temporary
// directory, so every wire event comes from this file rather than a model.

import assert from "node:assert/strict";
import { chmod, mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { spawn } from "node:child_process";

const root = await mkdtemp("/private/tmp/boop-agent-control-buy-");
const home = join(root, "home");
const work = join(root, "work");
const agent = join(root, "fake-acp-agent.mjs");
const fakeBin = join(root, "bin");

await Promise.all([mkdir(home), mkdir(work), mkdir(fakeBin)]);

const fakeAgent = String.raw`
import readline from "node:readline";
const sessions = new Map();
const permissions = new Map();
let nextSession = 0;
let nextPermission = 0;
const send = (message) => process.stdout.write(JSON.stringify({ jsonrpc: "2.0", ...message }) + "\n");
const text = (prompt) => prompt.map((part) => part.text ?? "").join("");
function finish(promptId, session, reason) {
  if (session.finished) return;
  session.finished = true;
  send({ id: promptId, result: { stopReason: reason } });
}
async function prompt(message) {
  const session = sessions.get(message.params.sessionId);
  if (!session) return send({ id: message.id, error: { code: -32000, message: "unknown session" } });
  session.finished = false;
  session.promptId = message.id;
  const body = text(message.params.prompt);
  if (body === "hold") {
    session.timer = setTimeout(() => finish(message.id, session, "end_turn"), 10_000);
    return;
  }
  send({ method: "session/update", params: { sessionId: message.params.sessionId, update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "fake reply: " + body } } } });
  if (!body.includes("permission")) return finish(message.id, session, "end_turn");
  const permissionId = "permission-" + (++nextPermission);
  const answer = new Promise((resolve) => permissions.set(permissionId, resolve));
  send({ id: permissionId, method: "session/request_permission", params: { sessionId: message.params.sessionId, toolCall: { toolCallId: "fake-edit", title: "fake edit", kind: "edit", status: "pending", locations: [], rawInput: {} }, options: [{ optionId: "allow", name: "allow", kind: "allow_once" }] } });
  await answer;
  send({ method: "session/update", params: { sessionId: message.params.sessionId, update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "permission response: " + JSON.stringify(answer) } } } });
  finish(message.id, session, "end_turn");
}
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  if (message.id && !message.method && permissions.has(String(message.id))) {
    permissions.get(String(message.id))();
    permissions.delete(String(message.id));
    return;
  }
  if (message.method === "initialize") return send({ id: message.id, result: { protocolVersion: 1, agentCapabilities: { loadSession: false } } });
  if (message.method === "session/new") {
    const sessionId = "fake-session-" + (++nextSession);
    sessions.set(sessionId, {});
    return send({ id: message.id, result: { sessionId } });
  }
  if (message.method === "session/prompt") return void prompt(message);
  if (message.method === "session/cancel") {
    const session = sessions.get(message.params.sessionId);
    if (session?.timer) clearTimeout(session.timer);
    if (session) finish(session.promptId, session, "cancelled");
  }
});
`;

await writeFile(agent, fakeAgent);
const fakeCodexAcp = join(fakeBin, "codex-acp");
await writeFile(fakeCodexAcp, `#!${process.execPath}\nimport ${JSON.stringify(agent)};\n`);
await chmod(fakeCodexAcp, 0o755);
const acpx = process.env.ACPX_BIN ?? "acpx";
const agentCommand = `${process.execPath} ${agent}`;
const base = ["--agent", agentCommand, "--cwd", work];
const environment = { ...process.env, HOME: home };

function invoke(args) {
  return new Promise((resolve, reject) => {
    const child = spawn(acpx, [...base, ...args], { env: environment });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (data) => { stdout += data; });
    child.stderr.on("data", (data) => { stderr += data; });
    child.on("error", reject);
    child.on("close", (code) => code === 0 ? resolve({ stdout, stderr }) : reject(new Error(`${args.join(" ")} exited ${code}: ${stderr}`)));
  });
}

function invokeAgentTeam(args) {
  const agentTeam = process.env.AGENT_TEAM_BIN ?? "agent-team";
  return new Promise((resolve, reject) => {
    const child = spawn(agentTeam, args, { env: { ...environment, PATH: `${fakeBin}:${process.env.PATH}` } });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (data) => { stdout += data; });
    child.stderr.on("data", (data) => { stderr += data; });
    child.on("error", reject);
    child.on("close", (code) => code === 0 ? resolve({ stdout, stderr }) : reject(new Error(`agent-team ${args.join(" ")} exited ${code}: ${stderr}`)));
  });
}

function invokeSystem(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (data) => { stdout += data; });
    child.stderr.on("data", (data) => { stderr += data; });
    child.on("error", reject);
    child.on("close", (code) => code === 0 ? resolve({ stdout, stderr }) : reject(new Error(`${command} ${args.join(" ")} exited ${code}: ${stderr}`)));
  });
}

const started = process.hrtime.bigint();
const oneShot = await invoke(["--approve-all", "--format", "json", "exec", "permission-first"]);
const coldMs = Number(process.hrtime.bigint() - started) / 1_000_000;

assert.match(oneShot.stdout, /"method":"initialize"/);
assert.match(oneShot.stdout, /session\/request_permission/);
assert.match(oneShot.stdout, /fake reply: permission-first/);
assert.match(oneShot.stdout, /"stopReason":"end_turn"/);
const receipt = {
  probe: "acpx-fake-acp-exec",
  provider_calls: 0,
  cold_ms: Number(coldMs.toFixed(1)),
  direct_stdio: true,
  permission_policy: "approve-all",
  tmux_or_pty: false,
};

if (process.env.ACPX_PERSISTENT === "1") {
  await invoke(["sessions", "new", "--name", "probe"]);
  const first = await invoke(["--approve-all", "--format", "json", "prompt", "-s", "probe", "permission-first"]);
  const second = await invoke(["--approve-all", "--format", "json", "prompt", "-s", "probe", "second"]);
  const held = invoke(["--format", "json", "prompt", "-s", "probe", "hold"]);
  await new Promise((resolve) => setTimeout(resolve, 200));
  await invoke(["cancel", "-s", "probe"]);
  const cancelled = await held;
  const history = await invoke(["sessions", "read", "probe"]);
  assert.match(first.stdout, /session\/request_permission/);
  assert.match(second.stdout, /fake reply: second/);
  assert.match(cancelled.stdout, /"stopReason":"cancelled"/);
  assert.match(history.stdout, /permission-first/);
  assert.match(history.stdout, /second/);
  receipt.probe = "acpx-fake-acp-persistent";
  receipt.named_session = true;
  receipt.persistence = true;
  receipt.cancellation = "cancelled";
}

if (process.env.AGENT_TEAM === "1") {
  try {
    const alphaAdded = await invokeAgentTeam(["add", "codex", "--name", "alpha", "--cwd", work, "--background"]);
    const betaAdded = await invokeAgentTeam(["add", "codex", "--name", "beta", "--cwd", work, "--background"]);
    const alphaPid = alphaAdded.stdout.match(/pid: (\d+)/)?.[1];
    const betaPid = betaAdded.stdout.match(/pid: (\d+)/)?.[1];
    assert.ok(alphaPid);
    assert.ok(betaPid);
    const alphaRss = Number((await invokeSystem("ps", ["-o", "rss=", "-p", alphaPid])).stdout.trim());
    const betaRss = Number((await invokeSystem("ps", ["-o", "rss=", "-p", betaPid])).stdout.trim());
    const first = await invokeAgentTeam(["ask", "alpha", "first"]);
    const second = await invokeAgentTeam(["ask", "alpha", "second"]);
    const waitingAllow = invokeAgentTeam(["ask", "alpha", "permission-allow"]);
    await new Promise((resolve) => setTimeout(resolve, 100));
    const beforeAllow = await invokeAgentTeam(["info", "alpha"]);
    const allow = await invokeAgentTeam(["allow", "alpha"]);
    await waitingAllow;
    const waitingDeny = invokeAgentTeam(["ask", "alpha", "permission-deny"]);
    await new Promise((resolve) => setTimeout(resolve, 100));
    const deny = await invokeAgentTeam(["deny", "alpha"]);
    await waitingDeny;
    const holding = invokeAgentTeam(["ask", "alpha", "hold"]);
    await new Promise((resolve) => setTimeout(resolve, 100));
    const parallel = await invokeAgentTeam(["ask", "beta", "parallel"]);
    const cancel = await invokeAgentTeam(["cancel", "alpha"]);
    await holding;
    const history = await invokeAgentTeam(["log", "alpha", "--last", "0"]);
    const listed = await invokeAgentTeam(["ls"]);
    assert.match(first.stdout, /fake reply: first/);
    assert.match(second.stdout, /fake reply: second/);
    assert.match(beforeAllow.stdout, /Pending:\s+1/);
    assert.match(allow.stdout, /Approved/);
    assert.match(deny.stdout, /Denied/);
    assert.match(parallel.stdout, /fake reply: parallel/);
    assert.match(cancel.stdout, /Cancel sent/);
    assert.match(history.stdout, /permission response: \{\}/);
    assert.match(listed.stdout, /alpha/);
    assert.match(listed.stdout, /beta/);
    assert.ok(alphaRss > 0);
    assert.ok(betaRss > 0);
    receipt.agent_team = {
      provider_calls: 0,
      named_workers: ["alpha", "beta"],
      idle_rss_kib: { alpha: alphaRss, beta: betaRss },
      second_prompt_persistence: true,
      permission: "allow-and-deny",
      concurrent_workers: "beta completed while alpha hold was queued",
      cancellation: "request-sent",
    };
  } finally {
    await invokeAgentTeam(["rm", "--all"]);
  }
}

console.log(JSON.stringify(receipt));
