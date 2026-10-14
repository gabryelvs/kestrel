import { expect, it } from 'vitest';
import { strings } from '../strings';
import { TABS } from './TitleBar';

it('puts the Hardware tab between Health and Tweaks', () => {
  expect(TABS.map((tab) => strings.tabs[tab])).toEqual(['Home', 'Health', 'Hardware', 'Tweaks', 'Lock In', 'Benchmark', 'Toolbox']);
});
