import { describe, expect, it } from 'vitest';
import { flagText, graphShape, metricText, passLabel, pct, sessionHeadline, settingsText, spreadText, verdictTone } from './bench';
import type { BenchState, ValorantSettings } from './types';

const base: BenchState = {
  phase: 'waiting', profileId: 'valorant', pass: 0, gameRunning: true, runs: [null, null, null], resultId: null,
  startMode: 'hotkey', startKey: 'F10', captureSeconds: 90, switchAtSeconds: 30, secondsLeft: null, countdown: null,
};

describe('bench helpers', () => {
  it('formats metrics with their units, and the recorded settings', () => {
    expect(metricText('avg-fps', 412.4)).toBe('412');
    expect(metricText('low01-fps', 201.6)).toBe('202');
    expect(metricText('pc-latency-ms', 9.84)).toBe('9.8 ms');
    expect(metricText('pc-latency-ms', null)).toBe('Not reported');
    expect(metricText('stutters-per-min', 0.44)).toBe('0.4/min');
    expect(spreadText(0.0317)).toBe('3.2%');
    const settings: ValorantSettings = {
      resolution: [1920, 1080], fullscreenMode: 0, vsync: false, frameRateLimit: 0, reflex: 2, multithreadedRendering: true,
      quality: { TextureQuality: '0' }, limits: {},
    };
    expect(settingsText(null)).toBe('Settings not found');
    expect(settingsText(settings)).toBe('1920×1080 · V-Sync off · No FPS limit');
    const unknown = { ...settings, resolution: null, fullscreenMode: null, vsync: null, frameRateLimit: null, reflex: null, multithreadedRendering: null };
    expect(settingsText(unknown)).toBe('—');
  });

  it('signs percentages and colours verdicts', () => {
    expect(pct(4.94)).toBe('+4.9%');
    expect(pct(-1.04)).toBe('-1.0%');
    expect(pct(0)).toBe('0.0%');
    expect(verdictTone('real-gain')).toBe('good');
    expect(verdictTone('real-loss')).toBe('bad');
    expect(verdictTone('noise')).toBe('neutral');
  });

  it('explains every flag', () => {
    expect(flagText({ kind: 'busy-background', name: 'chrome.exe' })).toBe('chrome.exe was busy in the background.');
    for (const kind of ['too-short', 'not-in-front', 'fps-capped', 'dropped-data'] as const) {
      expect(flagText({ kind }).length).toBeGreaterThan(10);
    }
  });

  it('names passes', () => {
    expect(passLabel(0)).toBe('Warm-up');
    expect(passLabel(2)).toBe('Run 2 of 3');
  });

  it('tells the player what to do next', () => {
    expect(sessionHeadline({ ...base, gameRunning: false })).toBe('Start VALORANT and open the Range. Kestrel waits for the game.');
    expect(sessionHeadline(base)).toBe('Stand at the start and press F10 to begin the warm-up.');
    expect(sessionHeadline({ ...base, pass: 2 })).toBe('Stand at the start and press F10 to begin run 2 of 3.');
    expect(sessionHeadline({ ...base, startMode: 'countdown' })).toContain('click Start run');
    expect(sessionHeadline({ ...base, countdown: 4 })).toBe('Starting in 4…');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 75 })).toBe('Run 1 of 3: walk the main path to the bots.');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 50 })).toBe('Run 1 of 3: shoot the strafing bots.');
    expect(sessionHeadline({ ...base, phase: 'review', pass: 1, flags: [{ kind: 'too-short' }] })).toBe('Run 1 of 3 can’t be trusted.');
    expect(sessionHeadline({ ...base, phase: 'done', pass: 3 })).toBe('Benchmark saved.');
    expect(sessionHeadline({ ...base, phase: 'failed', message: 'x' })).toBe('The benchmark stopped.');
    expect(sessionHeadline({ ...base, phase: 'cancelled' })).toBe('Benchmark cancelled.');
  });

  it('scales the frame-time graph to its slowest frame', () => {
    const g = graphShape([2, 4, 2, 8]);
    expect(g.points).toBe('0,75 200,50 400,75 600,0');
    expect(g.maxMs).toBe(8);
    expect(g.medianMs).toBe(2);
    expect(graphShape([]).points).toBe('');
    expect(graphShape([5]).points).toBe('0,0 600,0');
  });
});
