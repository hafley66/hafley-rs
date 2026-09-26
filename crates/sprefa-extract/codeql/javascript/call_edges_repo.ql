/** TypeScript explicit calls with a statically resolved corpus target. */
import javascript

Function owner(CallExpr site) {
  result = site.getParent+() and
  not exists(Function nearer |
    nearer = site.getParent+() and nearer.getParent+() = result
  )
}

string ownerName(CallExpr site) {
  result = owner(site).getName()
  or
  not exists(Function source | source = owner(site)) and result = "<module>"
}

from CallExpr site, Function target
where
  target = site.getResolvedCallee() and
  (exists(Function source | source = owner(site)) or exists(target.getBody())) and
  exists(site.getFile().getRelativePath()) and
  exists(target.getFile().getRelativePath())
select site.getFile().getRelativePath() as src_file,
  ownerName(site) as enclosing_item,
  target.getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
