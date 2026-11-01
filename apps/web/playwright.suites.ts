// Backend ownership for browser flows. The full test target runs every group.
export const durableBrowserSpecs = [
  'streamed-balance.spec.ts', 'commercial-models.spec.ts', 'balance-stress.spec.ts',
  'model-contracts.spec.ts', 'market-indexed.spec.ts', 'mortgage-closing-capital.spec.ts',
  'modeled-product-ftp.spec.ts', 'saved-derivative-mixed.spec.ts',
  'joint-deposit-derivative.spec.ts', 'hedge-accounting.spec.ts',
];
export const cohortBrowserSpecs = ['cohorts.spec.ts', 'treasury.spec.ts', 'native-settings.spec.ts'];
export const nativeBrowserSpecs = ['liquidity-lifecycle.spec.ts'];
