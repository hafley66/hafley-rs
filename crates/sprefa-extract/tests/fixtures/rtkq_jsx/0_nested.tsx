function Card({ title }: { title: string }) {
  return <article title={title}><span>{title}</span></article>;
}

const Panel = ({ title, ...props }: { title: string }) => (
  <section id="panel" hidden {...props}>
    <Card title={title} />
    <UI.Badge count={1} />
    <><footer data-label="end" /></>
  </section>
);

function Calls() {
  // useCommentOnlyQuery() and <Fake ignored /> are not syntax.
  const text = "useStringOnlyMutation() <Fake />";
  function Inner() { return api.fetch(text); }
  const load = () => useNestedQuery();
  return factory()();
}
