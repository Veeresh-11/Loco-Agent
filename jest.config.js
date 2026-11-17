module.exports = {
  preset: 'ts-jest',
  testEnvironment: 'node',
  roots: ['<rootDir>/src'],
  testPathIgnorePatterns: ['/node_modules/', 'mockVscode'],
  moduleNameMapper: {
    '^vscode$': '<rootDir>/src/__tests__/mockVscode.ts'
  }
};
