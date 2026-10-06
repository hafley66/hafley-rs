function useRead() { while (active) { useOne(); if (done) return; useTwo(); } }
