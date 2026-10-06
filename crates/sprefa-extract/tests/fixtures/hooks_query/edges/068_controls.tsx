function App() { const event = useEffectEvent(() => {}); function Other(event) { event(); } function Wrong() { event(); } useEffect(() => { const event = ordinary; event(); }); }
function useLabels() { exit: { if (stop) break exit; useBroken(); } other: { break other; } useReachable(); }
