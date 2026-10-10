import { Fragment } from 'react';
import { cpuLabel, displayLabel, driverLabel, mainGpu, ramLabel } from '../api/findings';
import { osLabel } from '../api/format';
import type { SystemSnapshot } from '../api/types';
import { strings } from '../strings';

export function HardwarePanel({ snapshot: s }: { snapshot: SystemSnapshot }) {
  const gpu = mainGpu(s.gpus);
  const rows: [string, string][] = [
    [strings.hardware.cpu, cpuLabel(s)],
    [strings.hardware.gpu, gpu ? `${gpu.name.replace(/^(NVIDIA GeForce|AMD Radeon) /, '')} · ${driverLabel(gpu)}` : '—'],
    [strings.hardware.ram, ramLabel(s)],
    [strings.hardware.display, displayLabel(s)],
    [strings.hardware.os, osLabel(s)],
    [strings.hardware.board, s.board.product || '—'],
  ];
  return (
    <section className="panel">
      <div className="label">{strings.home.hardware}</div>
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
