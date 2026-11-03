// Run after search highlighting: splitting text earlier would change source offsets.
export function remarkSoftBreaks() {
  return (tree: any) => {
    function visit(node: any) {
      if (!Array.isArray(node.children)) return;
      node.children = node.children.flatMap((child: any) => {
        if (child.type !== 'text') { visit(child); return [child]; }
        const lines = child.value.split('\n');
        return lines.flatMap((value: string, index: number) =>
          index ? [{type: 'break'}, {type: 'text', value}] : [{type: 'text', value}]);
      });
    }
    visit(tree);
  };
}
