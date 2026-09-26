/** TypeScript explicit calls with a statically resolved corpus target. */
import javascript

Function owner(CallExpr site) {
  result = site.getParent+() and
  not exists(Function nearer |
    nearer = site.getParent+() and nearer.getParent+() = result
  )
}

from CallExpr site, Function target, Function source
where
  target = site.getResolvedCallee() and
  source = owner(site) and
  exists(site.getFile().getRelativePath()) and
  exists(target.getFile().getRelativePath())
select site.getFile().getRelativePath() as src_file,
  source.getName() as enclosing_item,
  target.getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
