function useOuter() {
  const whatever = function useInner() { useState(); };
  let useAssigned; useAssigned = () => useState();
  const obj = { useProperty: () => useState(), useMethod() { useState(); } };
  const { useDefault = () => useState() } = {};
  ({ useDefault = () => useState() } = {});
}
