import { Fragment } from 'react';
import { cpuLabel, displayLabel, driverLabel, mainGpu, ramLabel } from '../api/findings';
import { osLabel } from '../api/format';
import { gpuShortName } from '../api/hardware';
import type { SystemSnapshot } from '../api/types';
import { strings } from '../strings';
import type { Tab } from './TitleBar';

export function HardwarePanel({ snapshot: s, onTab }: { snapshot: SystemSnapshot; onTab: (tab: Tab) => void }) {
  const gpu = mainGpu(s.gpus);
  const rows: [string, string][] = [
    [strings.hardware.cpu, cpuLabel(s)],
    [strings.hardware.gpu, gpu ? `${gpuShortName(gpu.name)} · ${driverLabel(gpu)}` : '—'],
    [strings.hardware.ram, ramLabel(s)],
    [strings.hardware.display, displayLabel(s)],
    [strings.hardware.os, osLabel(s)],
    [strings.hardware.board, s.board.product || '—'],
  ];
  return (
    <section className="panel">
      <div className="panel-head">
        <span className="label">{strings.home.hardware}</span>
        <button className="link" onClick={() => onTab('hardware')}>{strings.home.seeHardware}</button>
      </div>
      <dl className="hw num">
        {rows.map(([name, value]) => (
          <Fragment key={name}>
            <dt>{name}</dt>
            <dd title={value}>{value}</dd>
          </Fragment>
        ))}
      </dl>
    </section>
  );
}
