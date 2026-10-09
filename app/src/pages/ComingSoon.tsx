import { strings } from '../strings';

export function ComingSoon({ stage, title, body }: { stage: number; title: string; body: string }) {
  return (
    <section className="panel coming-soon">
      <div className="label">{strings.comingSoon.arrives(stage)}</div>
      <h1>{title}</h1>
      <p className="muted">{body}</p>
    </section>
  );
}
