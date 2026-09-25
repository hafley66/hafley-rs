import { named } from './named';
export * from './star';
export { relay } from './relay';
import './effect';
export async function load() {
  return import('./lazy');
}
const legacy = require('./legacy');
export const all = [named, legacy];
