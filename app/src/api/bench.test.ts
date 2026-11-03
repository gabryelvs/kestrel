import { describe, expect, it } from 'vitest';
import {
  canStartRun, contextRows, flagText, graphShape, metricText, passLabel, pct, sessionHeadline, settingsText, showsSeeYourPlan, spreadText, tweakList, tweakName, variesTooMuch,
  verdictTone,
} from './bench';
import type { BenchContext, BenchState, ValorantSettings } from './types';

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

  it('lists the PC at the time, with the PCIe link measured under load', () => {
    const ctx: BenchContext = {
      powerPlan: 'EXM Free Power Plan V8', ramMts: 3600, gpu: 'NVIDIA GeForce RTX 3060 Ti', gpuDriver: '610.74', windowsBuild: 26200,
      displayHz: 360, gameSettings: null, gpuPnpDeviceId: 'PCI\\VEN_10DE&DEV_2489', gpuLink: { gen: 4, width: 16 }, appliedTweaks: null,
    };
    expect(contextRows(ctx)).toEqual([
      ['Power plan', 'EXM Free Power Plan V8'], ['RAM', '3600 MT/s'], ['Graphics', 'NVIDIA GeForce RTX 3060 Ti · 610.74'],
      ['PCIe link under load', '4.0 x16'], ['Windows build', '26200'], ['Monitor', '360 Hz'], ['VALORANT', 'Settings not found'],
    ]);
    // Saved by v0.2.0, or Windows didn't report the link during the runs.
    expect(contextRows({ ...ctx, gpuPnpDeviceId: null, gpuLink: null })[3]).toEqual(['PCIe link under load', 'Not reported']);
  });

  it('flags runs that vary by more than 10%, as kestrel-bench does', () => {
    expect(spreadText(0.249)).toBe('24.9%');
    expect(variesTooMuch(0.249)).toBe(true);
    expect(variesTooMuch(0.1001)).toBe(true);
    expect(variesTooMuch(0.1)).toBe(false);
    expect(variesTooMuch(0.0999)).toBe(false);
    expect(variesTooMuch(0.05)).toBe(false);
    expect(variesTooMuch(0)).toBe(false);
    expect(variesTooMuch(Number.NaN)).toBe(false);
  });

  it('names a tweak by its title, and an id it does not know by the id itself', () => {
    expect(tweakName('power-plan')).toBe('Kestrel power plan');
    expect(tweakName('game-mode')).toBe('Game Mode');
    expect(tweakName('system-protection')).toBe('System Protection');
    // From an older or a newer Kestrel: shown as saved, never dropped and never a crash.
    expect(tweakName('from-the-future')).toBe('from-the-future');
    expect(tweakName('')).toBe('');
    expect(tweakName('Game Mode')).toBe('Game Mode');
    // Names every object has are not titles.
    for (const id of ['constructor', 'toString', '__proto__', 'hasOwnProperty', 'valueOf']) expect(tweakName(id)).toBe(id);
    expect(tweakList(['game-mode', 'from-the-future', 'power-plan'])).toBe('Game Mode, from-the-future, Kestrel power plan');
    expect(tweakList([])).toBe('');
  });

  it('signs percentages and colours verdicts', () => {
    expect(pct(4.94)).toBe('+4.9%');
    expect(pct(-1.04)).toBe('-1.0%');
    expect(pct(0)).toBe('0.0%');
    expect(pct(0.04)).toBe('0.0%');
    expect(pct(-0.04)).toBe('0.0%');
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
    // F10 is the way to start, with the button named as the way out for a game that swallows F10.
    expect(sessionHeadline(base)).toBe('Stand at the start and press F10 to begin the warm-up. If F10 does nothing in game, click Start run here: you get 10 seconds to switch back.');
    expect(sessionHeadline({ ...base, pass: 2 })).toBe('Stand at the start and press F10 to begin run 2 of 3. If F10 does nothing in game, click Start run here: you get 10 seconds to switch back.');
    expect(sessionHeadline({ ...base, startMode: 'countdown' })).toContain('click Start run');
    expect(sessionHeadline({ ...base, countdown: 4 })).toBe('Starting in 4…');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 75 })).toBe('Run 1 of 3: walk the main path to the bots.');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: 50 })).toBe('Run 1 of 3: shoot the strafing bots.');
    expect(sessionHeadline({ ...base, phase: 'capturing', pass: 1, secondsLeft: null })).toBe('Reading the results…');
    expect(sessionHeadline({ ...base, phase: 'review', pass: 1, flags: [{ kind: 'too-short' }] })).toBe('Run 1 of 3 can’t be trusted.');
    expect(sessionHeadline({ ...base, phase: 'done', pass: 3 })).toBe('Benchmark saved.');
    expect(sessionHeadline({ ...base, phase: 'failed', message: 'x' })).toBe('The benchmark stopped.');
    expect(sessionHeadline({ ...base, phase: 'cancelled' })).toBe('Benchmark cancelled.');
  });

  it('offers Start run while a pass waits with the game running, in both start modes', () => {
    // A game can swallow F10 even though Kestrel holds it, so the button isn't only for the countdown mode.
    expect(canStartRun(base)).toBe(true);
    expect(canStartRun({ ...base, startMode: 'countdown' })).toBe(true);
    expect(canStartRun({ ...base, pass: 3 })).toBe(true);
    expect(canStartRun({ ...base, gameRunning: false })).toBe(false);
    for (const startMode of ['hotkey', 'countdown'] as const) {
      expect(canStartRun({ ...base, startMode, countdown: 7 })).toBe(false);
      expect(canStartRun({ ...base, startMode, phase: 'capturing', pass: 1, secondsLeft: 60 })).toBe(false);
      expect(canStartRun({ ...base, startMode, phase: 'review', pass: 1, flags: [{ kind: 'too-short' }] })).toBe(false);
      expect(canStartRun({ ...base, startMode, phase: 'done', pass: 3 })).toBe(false);
      expect(canStartRun({ ...base, startMode, phase: 'failed', message: 'x' })).toBe(false);
      expect(canStartRun({ ...base, startMode, phase: 'cancelled' })).toBe(false);
    }
  });

  it('offers See your plan on the first saved benchmark only, and only while the plan note is wanted', () => {
    expect(showsSeeYourPlan(1, true)).toBe(true);
    // Nothing saved yet, or the list is not read yet: there is no result to put it on.
    expect(showsSeeYourPlan(0, true)).toBe(false);
    // The second result and every later one.
    for (const saved of [2, 3, 20]) expect(showsSeeYourPlan(saved, true), `${saved} saved`).toBe(false);
    // The first run was done before the note existed, or the note was seen: nobody is led anywhere.
    for (const saved of [0, 1, 2, 20]) expect(showsSeeYourPlan(saved, false), `${saved} saved, not wanted`).toBe(false);
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
