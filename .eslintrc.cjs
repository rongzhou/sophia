/* eslint-env node */
module.exports = {
  root: true,
  env: { node: true, es2022: true },
  parser: '@typescript-eslint/parser',
  parserOptions: { sourceType: 'module', ecmaVersion: 'latest' },
  plugins: ['@typescript-eslint'],
  extends: [
    'eslint:recommended',
    'plugin:@typescript-eslint/recommended',
    'prettier',
  ],
  ignorePatterns: ['dist/', 'node_modules/', 'website/**', 'sophia-runs/**'],
  rules: {
    'no-console': 'off',
    'no-undef': 'off',
    'no-var': 'error',
    'prefer-const': 'warn',
    'eqeqeq': ['warn', 'always', { null: 'ignore' }],
    'complexity': ['warn', 15],
    '@typescript-eslint/no-explicit-any': 'warn',
    '@typescript-eslint/consistent-type-imports': 'warn',
    '@typescript-eslint/no-unused-vars': 'warn',
  },
};
