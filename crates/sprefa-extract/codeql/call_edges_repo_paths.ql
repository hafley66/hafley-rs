/** Partial root-call diagnostic: direct path calls only, excluding method and struct syntax. */
import rust
import codeql.rust.internal.PathResolution

from CallExpr site, Function target, Function source
where
  site.fromSource() and target.fromSource() and
  not site.isInMacroExpansion() and
  target = resolvePath(site.getFunction().(PathExpr).getPath()) and
  source = site.getEnclosingCallable() and
  exists(target.getLocation().getFile().getRelativePath()) and
  exists(site.getLocation().getFile().getRelativePath()) and
  exists(target.getName())
select site.getLocation().getFile().getRelativePath() as src_file,
  source.getName().getText() as enclosing_item,
  target.getLocation().getFile().getRelativePath() as dst_file,
  target.getName() as dst_name
