import { expect, it } from 'vitest';
import { sampleDetail, sampleSetBaseline } from './sample-bench';

// Its own file: sample-bench.test.ts deletes the samples, and module state is shared within a file.
it('records the tweaks each sample ran with, and says what changed from whichever one is the baseline', () => {
  expect(sampleDetail('sample-1').benchmark.context.appliedTweaks).toEqual([]);
  expect(sampleDetail('sample-2').benchmark.context.appliedTweaks).toEqual(['power-plan', 'game-mode']);

  expect(sampleDetail('sample-1').comparison).toBeNull();
  expect(sampleDetail('sample-2').comparison).toMatchObject({
    tweaksAdded: ['power-plan', 'game-mode'],
    tweaksRemoved: [],
    powerPlanChange: ['EXM Free Power Plan V8', 'Kestrel'],
  });

  // The other way round, the same two results swap sides.
  sampleSetBaseline('sample-2');
  expect(sampleDetail('sample-1').comparison).toMatchObject({
    tweaksAdded: [],
    tweaksRemoved: ['power-plan', 'game-mode'],
    powerPlanChange: ['Kestrel', 'EXM Free Power Plan V8'],
  });
});
