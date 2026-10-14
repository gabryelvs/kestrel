// The owner's PC as a full schema-2 report, for unit tests: the values of crates/kestrel-core/fixtures/owner-pc.json
// (less its `windows` and `background` parts, which the UI doesn't read), plus the link measured by a benchmark, the
// Health Check items the helpers read, and a scan time without a zone, so it reads the same local time on any machine.
// Tests build on this rather than on mock-report.json (the browser preview's sample), so they pin exact text whatever
// that sample holds. Each call returns a fresh copy, free to change.
import type { Finding, Report } from './types';

const check = (id: string, category: Finding['category'], severity: Finding['severity'], status: Finding['status'], title: string, detail = '', fix: Finding['fix'] = { kind: 'nothing' }): Finding =>
  ({ id, category, severity, status, title, detail, fix });

export function ownerReport(): Report {
  return {
    appVersion: '0.3.0',
    snapshot: {
      schemaVersion: 2,
      collectedAt: '2026-10-13T21:04:00',
      os: { caption: 'Microsoft Windows 11 Pro', build: 26200, displayVersion: '25H2' },
      machine: { formFactor: 'desktop', manufacturer: 'ASUS', model: '', onBattery: null },
      cpu: {
        name: '11th Gen Intel(R) Core(TM) i9-11900K @ 3.50GHz', vendor: 'intel', cores: 8, threads: 16,
        family: 6, model: 167, stepping: 1, codename: 'Rocket Lake-S', socket: 'LGA1200', baseMhz: 3500, maxMhz: 5300,
        pCores: null, eCores: null,
        caches: [
          { level: 1, kind: 'data', sizeKb: 48, count: 8 },
          { level: 1, kind: 'instruction', sizeKb: 32, count: 8 },
          { level: 2, kind: 'unified', sizeKb: 512, count: 8 },
          { level: 3, kind: 'unified', sizeKb: 16384, count: 1 },
        ],
        instructions: ['SSE4.2', 'AVX', 'AVX2', 'AVX-512'],
        virtualization: true,
      },
      gpus: [
        {
          name: 'Parsec Virtual Display Adapter', vendor: 'virtual', driverVersion: '0.45.0.0', driverDate: null, vramMb: null,
          isIntegrated: false, pnpDeviceId: 'ROOT\\DISPLAY\\0000', pcie: null, linkUnderLoad: null, largestBarMb: null, resizableBar: null,
        },
        {
          name: 'NVIDIA GeForce RTX 3060 Ti', vendor: 'nvidia', driverVersion: '32.0.16.1074', driverDate: '2026-09-15', vramMb: 8192,
          isIntegrated: false, pnpDeviceId: 'PCI\\VEN_10DE&DEV_2489&SUBSYS_884F1043&REV_A1\\4&2B0D3A0E&0&0008',
          pcie: { cardMax: { gen: 4, width: 16 }, slotMax: { gen: 4, width: 16 }, now: { gen: 1, width: 16 } },
          linkUnderLoad: { link: { gen: 4, width: 16 }, measuredAt: '2026-10-13T21:40:00', benchmarkId: 'sample-1' },
          largestBarMb: 8192, resizableBar: true,
        },
      ],
      memory: {
        totalMb: 32768, ddrGeneration: 4, slotsTotal: 4, ratedMts: 3600,
        modules: [
          { capacityMb: 16384, configuredMts: 3600, maxMts: 2133, partNumber: 'CMG32GX4M2D3600C18', manufacturer: 'Corsair', bankLabel: 'BANK 1', deviceLocator: 'ChannelA-DIMM2', rank: 2 },
          { capacityMb: 16384, configuredMts: 3600, maxMts: 2133, partNumber: 'CMG32GX4M2D3600C18', manufacturer: 'Corsair', bankLabel: 'BANK 3', deviceLocator: 'ChannelB-DIMM2', rank: 2 },
        ],
      },
      displays: [
        { name: '\\\\.\\DISPLAY1', adapter: 'NVIDIA GeForce RTX 3060 Ti', currentHz: 360, maxHz: 360, width: 1920, height: 1080, isPrimary: true, isVirtual: false, monitorName: 'XL2566K', connection: 'DisplayPort' },
        { name: '\\\\.\\DISPLAY2', adapter: 'NVIDIA GeForce RTX 3060 Ti', currentHz: 180, maxHz: 180, width: 1920, height: 1080, isPrimary: false, isVirtual: false, monitorName: '24G4', connection: 'DisplayPort' },
      ],
      board: {
        manufacturer: 'ASUSTeK COMPUTER INC.', product: 'ROG MAXIMUS XIII HERO', biosVersion: '2103', biosDate: '2024-05-20',
        biosVendor: 'American Megatrends Inc.', chipset: 'Intel Z590', bootMode: 'uefi',
      },
      storage: {
        valorant: { path: 'C:/Riot Games/VALORANT/live', driveLetter: 'C', media: 'nvme', freeGb: 210.5, totalGb: 931.5 },
        drives: [
          { model: 'Samsung SSD 980 PRO 1TB', media: 'nvme', bus: 'nvme', sizeGb: 931.5, volumes: [{ letter: 'C', freeGb: 210.5, totalGb: 931.5 }] },
          { model: 'WDC WD20EZBX-00AYRA0', media: 'hdd', bus: 'sata', sizeGb: 1863, volumes: [{ letter: 'D', freeGb: 1228.8, totalGb: 1863 }] },
        ],
      },
      security: { tpm2: true, secureBoot: true, vbsRunning: false, hvciRunning: false, iommu: false, vanguard: 'running' },
      network: {
        adapters: [{ name: 'Intel(R) Ethernet Controller (3) I225-V', kind: 'ethernet', linkMbps: 2500, connected: true, powerSaving: true }],
        deliveryOptimizationInternet: false,
      },
      probeErrors: [],
    },
    findings: [
      check('ram-speed', 'hardware', 'high', 'pass', 'RAM runs at 3600 MT/s, its rated speed', 'The kit is rated 3600 MT/s.'),
      check('ram-channels', 'hardware', 'high', 'pass', 'RAM runs in dual channel'),
      check('bios-age', 'hardware', 'low', 'problem', 'BIOS is from 2024-05-20', '', { kind: 'bios-guide', topic: 'update-bios' }),
      check('pcie-lanes', 'hardware', 'medium', 'pass', 'Graphics card uses all its PCIe lanes', 'x16, as the card supports.'),
      check('pcie-speed', 'hardware', 'low', 'pass', 'PCIe link runs at full speed under load', 'PCIe 4.0 during your latest benchmark.'),
      check('resizable-bar', 'hardware', 'low', 'pass', 'Resizable BAR is on'),
      check('tpm', 'vanguard', 'high', 'pass', 'TPM 2.0 is on'),
      check('secure-boot', 'vanguard', 'high', 'pass', 'Secure Boot is on'),
      check('virtualization', 'vanguard', 'info', 'pass', 'Virtualization is on'),
      check('iommu', 'vanguard', 'info', 'info', 'IOMMU is off', '', { kind: 'bios-guide', topic: 'iommu' }),
      check('adapter-power', 'network', 'low', 'problem', 'Network adapter power saving is on', '', { kind: 'later-stage', stage: 6 }),
    ],
    score: { value: 86, problems: 3, potentialGain: 14 },
    guide: { vendor: 'asus', board: 'ROG MAXIMUS XIII HERO', enterKey: 'Del', tip: '', sections: [] },
  };
}

/** The report with some Health Check items changed, by id. The one helper every test that needs a different check shares. */
export function withFindings(report: Report, changes: Record<string, Partial<Finding>>): Report {
  return { ...report, findings: report.findings.map((f) => ({ ...f, ...changes[f.id] })) };
}
