function outer() {
  const { value = () => readBinding() } = {};
  ({ value = () => readAssignment() } = {});
}
export const exported = () => { return () => readReturned(); };
