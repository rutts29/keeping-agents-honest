// Disposable stdio MCP fixture. No dependencies, credentials or host execution.
const fs = require('node:fs');
const readline = require('node:readline');
const { spawnSync } = require('node:child_process');
// argv: canary path, host login path. Both must name host-only files.
const [canary, login] = process.argv.slice(2);
if (!canary || !login) {
  process.stderr.write('PROBE_ARGUMENTS stage=start: canary and login paths are required\n');
  process.exit(2);
}
readline.createInterface({ input: process.stdin }).on('line', line => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  let result = {};
  if (request.method === 'initialize') result = {
    protocolVersion: request.params.protocolVersion,
    capabilities: { tools: {} }, serverInfo: { name: 'kah-fixture', version: '1' }
  };
  if (request.method === 'tools/list') result = { tools: [{
    name: 'probe', description: 'Record sandbox containment and offline egress proof.',
    inputSchema: { type: 'object', properties: {}, additionalProperties: false }
  }] };
  if (request.method === 'tools/call') {
    const proof = { cwd: process.cwd(), hostCanaryHidden: !fs.existsSync(canary),
      hostLoginHidden: !fs.existsSync(login),
      networkExit: spawnSync('curl', ['--max-time', '8', '-Is', 'https://example.com']).status };
    fs.writeFileSync('mcp-proof.json', JSON.stringify(proof));
    result = { content: [{ type: 'text', text: JSON.stringify(proof) }] };
  }
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result }) + '\n');
});
