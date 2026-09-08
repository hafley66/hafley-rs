import { SyntaxKind, visitChildren } from '@typespec/compiler/ast';

// TypeSpec 1.10 does not expose constants on Namespace. Isolate its AST/internal
// value-checker access here; pinned-compiler generation tests cover this seam.
export function constants(program, source) {
  const result = {};
  function visit(node) {
    if (node.kind === SyntaxKind.ConstStatement) {
      const value = program.checker.getValueForNode(node);
      if (value?.valueKind === 'StringValue' && value.type.name === 'string') {
        if (result[node.id.sv]) throw Error(`duplicate constant: ${node.id.sv}`);
        result[node.id.sv] = { type: 'string', value: value.value };
        return;
      }
      if (value?.valueKind !== 'NumericValue' || !['int64', 'uint32', 'uint64'].includes(value.type.name)) {
        throw Error(`unsupported constant: ${node.id.sv}`);
      }
      if (!value.value.isInteger) throw Error(`constant must be integral: ${node.id.sv}`);
      if (result[node.id.sv]) throw Error(`duplicate constant: ${node.id.sv}`);
      // Decimal text preserves all 64 bits, including values beyond JS precision.
      result[node.id.sv] = { type: value.type.name, value: value.value.toString() };
    }
    visitChildren(node, visit);
  }
  visit(program.sourceFiles.get(source));
  return result;
}
