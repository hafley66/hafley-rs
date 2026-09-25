// Generated from sprefa-extract/queries/kotlin/scip.scm by sprefa-extract/build.rs
// (OUT_DIR/kotlin_scm.rs); copy that output here when the query changes.
pub struct CallDef<'a> { fact: &'a hafley_scm::EmittedFact, arena: &'a hafley_scm::MatchArena }
impl<'a> CallDef<'a> {
  pub fn rows(arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { arena.emitted.iter().filter(|fact| fact.relation == 0).map(move |fact| Self { fact, arena }) }
  pub fn span(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 0).expect("SCM emitted required field") }
  pub fn name(&self) -> Option<&'a hafley_scm::EmittedValue> { self.fact.get(self.arena, 1) }
  pub fn kind(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 2).expect("SCM emitted required field") }
  pub fn body(&self) -> Option<&'a hafley_scm::EmittedValue> { self.fact.get(self.arena, 3) }
}
pub struct CallScope<'a> { fact: &'a hafley_scm::EmittedFact, arena: &'a hafley_scm::MatchArena }
impl<'a> CallScope<'a> {
  pub fn rows(arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { arena.emitted.iter().filter(|fact| fact.relation == 1).map(move |fact| Self { fact, arena }) }
  pub fn span(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 0).expect("SCM emitted required field") }
  pub fn name(&self) -> Option<&'a hafley_scm::EmittedValue> { self.fact.get(self.arena, 1) }
  pub fn kind(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 2).expect("SCM emitted required field") }
}
pub struct CallSite<'a> { fact: &'a hafley_scm::EmittedFact, arena: &'a hafley_scm::MatchArena }
impl<'a> CallSite<'a> {
  pub fn rows(arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { arena.emitted.iter().filter(|fact| fact.relation == 2).map(move |fact| Self { fact, arena }) }
  pub fn span(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 0).expect("SCM emitted required field") }
  pub fn group(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 4).expect("SCM emitted required field") }
  pub fn callee(&self) -> &'a hafley_scm::EmittedValue { self.fact.get(self.arena, 5).expect("SCM emitted required field") }
}
