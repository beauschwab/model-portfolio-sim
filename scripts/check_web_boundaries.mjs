/** TypeScript compiler AST boundary checks; no app code is imported/executed. */
import { createRequire } from 'node:module';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, relative, join } from 'node:path';
const require = createRequire(new URL('../apps/web/package.json', import.meta.url));
const ts = require('typescript');

export function violations(path, text) {
  const file = ts.createSourceFile(path, text, ts.ScriptTarget.Latest, true,
    path.endsWith('tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
  const errors = [];
  const add = (node, target, rule) => errors.push(`${path}:${file.getLineAndCharacterOfPosition(node.getStart(file)).line + 1}: ${target}: ${rule}`);
  const inspect = node => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      const target = node.moduleSpecifier?.text;
      if (target && /(?:^|\/)(?:packages|apps\/api|portfolio_risk)(?:\/|$)/.test(target)) add(node, target, 'web must depend on the API client, never engine/API implementation');
    }
    if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
      const expr = node.expression;
      const target = ts.isPropertyAccessExpression(expr) ? expr.name.text : ts.isIdentifier(expr) ? expr.text : '';
      const dynamic = target === 'require' || expr.kind === ts.SyntaxKind.ImportKeyword;
      if (dynamic && node.arguments?.[0] && ts.isStringLiteral(node.arguments[0]) && /(?:packages|apps\/api|portfolio_risk)/.test(node.arguments[0].text)) add(node,node.arguments[0].text,'web must depend on the API client, never engine/API implementation');
      if (['fetch', 'XMLHttpRequest', 'WebSocket', 'EventSource'].includes(target) && !path.replaceAll('\\','/').endsWith('src/lib/api.ts')) add(node,target,'HTTP and streaming transport belong in src/lib/api.ts');
    }
    ts.forEachChild(node, inspect);
  };
  inspect(file);
  return errors;
}

if (process.argv.includes('--self-test')) {
  for (const sample of ['fetch("/api")', 'window.fetch("/api")', 'new WebSocket("/api")', 'import x from "../../../packages/engine"', 'export { x } from "../../apps/api/x"', 'import("../../portfolio_risk/core")']) {
    if (!violations('src/pages/Fixture.tsx', sample).length) throw Error(`Missed forbidden fixture: ${sample}`);
  }
  if (violations('src/pages/Good.tsx', 'import { api } from "../lib/api"; // fetch("ignored")\nconst x = "fetch()";').length) throw Error('False positive in comments/string');
  if (violations('src/lib/api.ts','fetch("/api")').length) throw Error('Client transport must be allowed');
  console.log('TypeScript boundary fixtures verified');
} else {
  const root = resolve(process.argv[2] || '.');
  const walk = folder => readdirSync(folder, { withFileTypes: true }).flatMap(entry => entry.isDirectory() ? walk(join(folder,entry.name)) : /\.tsx?$/.test(entry.name) ? [join(folder,entry.name)] : []);
  const errors = walk(join(root,'apps/web/src')).flatMap(p=>violations(relative(root,p).replaceAll('\\','/'),readFileSync(p,'utf8')));
  if (errors.length) { console.error(errors.join('\n')); process.exitCode=1; }
  else console.log('TypeScript boundaries verified');
}
