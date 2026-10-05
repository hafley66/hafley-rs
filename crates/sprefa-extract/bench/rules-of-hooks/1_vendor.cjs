// Fixture extraction only. No tool invocation or comparison scoring.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');

const input = fs.readFileSync(path.join(__dirname, '0_ESLintRulesOfHooks-test.js'), 'utf8');
const digest = crypto.createHash('sha256').update(input).digest('hex');
if (digest !== '00fcca3f77f7b2982dc48fd90c14a02dcae38d9ac605ca7f7e90055343c4f080') {
  throw new Error('Pinned React test source changed');
}
// Evaluate the upstream case declarations and message helpers, before parser
// matrices, Jest registration, CI filtering, and deletion of syntax metadata.
const declarations = input.slice(0, input.indexOf('// For easier local testing\nif (!process.env.CI)'));
const tests = vm.runInNewContext(declarations + '\nallTests;', {
  require(name) {
    if (name === 'eslint-v7' || name === 'eslint-v9') return {RuleTester: class {}};
    if (name === 'eslint-plugin-react-hooks') return {default: {rules: {'rules-of-hooks': {}}}};
    throw new Error(`Unexpected upstream dependency: ${name}`);
  },
}, {timeout: 1000});
const cases = [];
for (const validity of ['valid', 'invalid']) {
  tests[validity].forEach((test, index) => {
    const id = `${validity}-${String(index).padStart(3, '0')}`;
    const directory = path.join(__dirname, 'fixtures', validity);
    fs.mkdirSync(directory, {recursive: true});
    const file = `fixtures/${validity}/${String(index).padStart(3, '0')}_Case.tsx`;
    fs.writeFileSync(path.join(__dirname, file), test.code);
    const {code, ...metadata} = test;
    cases.push({id, file, validity, ...metadata});
  });
}
fs.writeFileSync(path.join(__dirname, '1_cases.json'), JSON.stringify(cases, null, 2) + '\n');
console.log(JSON.stringify({valid: tests.valid.length, invalid: tests.invalid.length, source_sha256: digest}));
