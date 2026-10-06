async function outer() { readAsync(); function inner() { readSync(); } }
export async function exported() { readExport(); }
const arrow = async () => readArrow();
export default async function named() { readDefault(); }
