// Row accessors for the `scope.*` relations `scope/rust.scm` and `scope/ts.scm`
// emit. Ids are looked up by name per query: the two files intern in any order.
pub struct ScopeSchema { relations: [Option<u16>; 6], fields: [Option<u16>; 7] }
impl ScopeSchema {
  const RELATIONS: [&'static str; 6] = ["scope.open", "scope.decl", "scope.callable", "scope.output", "scope.async", "scope.use"];
  const FIELDS: [&'static str; 7] = ["span", "kind", "name", "type", "value", "capture", "mode"];
  pub fn of(query: &hafley_scm::QueryExt) -> Self {
    let find = |names: &[Box<str>], wanted: &str| names.iter().position(|name| name.as_ref() == wanted).map(|at| at as u16);
    Self { relations: Self::RELATIONS.map(|name| find(&query.relations, name)), fields: Self::FIELDS.map(|name| find(&query.fields, name)) }
  }
  fn rows<'a>(&self, relation: usize, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = &'a hafley_scm::EmittedFact> + 'a { let wanted = self.relations[relation]; arena.emitted.iter().filter(move |fact| Some(fact.relation) == wanted) }
}
pub struct ScopeRow<'a> { fact: &'a hafley_scm::EmittedFact, arena: &'a hafley_scm::MatchArena, schema: &'a ScopeSchema }
impl<'a> ScopeRow<'a> {
  pub fn open(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(0, arena).map(move |fact| Self { fact, arena, schema }) }
  pub fn decl(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(1, arena).map(move |fact| Self { fact, arena, schema }) }
  pub fn callable(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(2, arena).map(move |fact| Self { fact, arena, schema }) }
  pub fn output(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(3, arena).map(move |fact| Self { fact, arena, schema }) }
  pub fn is_async(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(4, arena).map(move |fact| Self { fact, arena, schema }) }
  pub fn uses(schema: &'a ScopeSchema, arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a { schema.rows(5, arena).map(move |fact| Self { fact, arena, schema }) }
  fn field(&self, field: usize) -> Option<&'a hafley_scm::EmittedValue> { self.schema.fields[field].and_then(|key| self.fact.get(self.arena, key)) }
  pub fn span(&self) -> &'a hafley_scm::EmittedValue { self.field(0).expect("SCM emitted required field") }
  pub fn kind(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(1) }
  pub fn name(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(2) }
  pub fn type_(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(3) }
  pub fn value(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(4) }
  pub fn capture(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(5) }
  pub fn mode(&self) -> Option<&'a hafley_scm::EmittedValue> { self.field(6) }
}
