import fs from 'node:fs';
import * as path from 'node:path';
import type { Node as TreeNode } from 'tree';
export function inspect(node: TreeNode): string {
  return fs.readFileSync(path.join(node.name, 'x'), 'utf8');
}
