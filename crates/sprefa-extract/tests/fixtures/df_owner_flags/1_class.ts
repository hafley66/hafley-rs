class Store { async load() { readMethod(); const callback = () => readCallback(); } field = async () => readField(); }
const object = { async load() { readObject(); } };
