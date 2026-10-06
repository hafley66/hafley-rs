jest.useFakeTimers();
function App() { useEffect(() => jest.useRealTimers()); }
