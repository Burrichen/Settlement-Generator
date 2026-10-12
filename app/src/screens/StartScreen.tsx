import { FiguresControls, RolesControls } from "../components/AuthorityFigures";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { generation, type GenerationResponse, type SessionView, type StageView } from "../api";
import { useApp } from "../app-state";
import {
  ActiveTags,
  ConfirmBar,
  OutcomeLine,
  ResultList,
  StageRail,
  TagList,
} from "../components/generation";
import { KeyHints } from "../components/ScreenFrame";
import { StageGrid } from "../components/StageGrid";
import { NameControls } from "../components/NameStage";
import { CompletedSettlement } from "./CompletedSettlement";

/**
 * Start: runs the flow the settings ask for. Every operation is a call into
 * the shared core; this screen only presents the returned `SessionView`.
 */
export function StartScreen() {
  const { home } = useApp();
  const [response, setResponse] = useState<GenerationResponse | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [finished, setFinished] = useState(false);
  const [confirmLeave, setConfirmLeave] = useState(false);

  const run = async (call: Promise<GenerationResponse>) => {
    setBusy(true);
    try {
      const next = await call;
      setResponse(next);
      return next;
    } catch (e) {
      setFatal(String(e));
      return null;
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void (async () => {
      const started = await run(generation.start());
      if (started?.view.mode === "automatic") {
        const ran = await run(generation.runAutomatic());
        if (ran?.view.complete) setFinished(true);
      }
    })();
  }, []);

  const view = response?.view;
  const anythingResolved = view?.stages.some((s) => s.status === "resolved") ?? false;

  const leave = async () => {
    await generation.discard().catch(() => undefined);
    home();
  };

  // Esc asks before throwing away generated results.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    if (confirmLeave) setConfirmLeave(false);
    else if (anythingResolved) setConfirmLeave(true);
    else void leave();
  };

  let body;
  if (fatal) {
    body = <FatalError message={fatal} onBack={leave} />;
  } else if (!view) {
    body = <p className="muted gen-loading">Preparing generation…</p>;
  } else if (finished && view.complete) {
    body = (
      <CompletedSettlement
        view={view}
        busy={busy}
        error={response?.error ?? null}
        onReroll={async () => {
          const next = await run(generation.rerollAll());
          if (next && !next.view.complete) setFinished(false);
        }}
        onSaved={home}
        onDiscarded={home}
      />
    );
  } else if (view.mode === "automatic") {
    body = (
      <AutomaticFlow
        view={view}
        error={response?.error ?? null}
        busy={busy}
        onChoose={async (stage, result) => {
          const chosen = await run(generation.choose(stage, result));
          if (chosen && !chosen.error) {
            const ran = await run(generation.runAutomatic());
            if (ran?.view.complete) setFinished(true);
          }
        }}
        onLeave={leave}
      />
    );
  } else if (view.mode === "incremental") {
    body = (
      <IncrementalFlow
        view={view}
        error={response?.error ?? null}
        busy={busy}
        onRoll={(i) => run(generation.roll(i))}
        onChoose={(i, r) => run(generation.choose(i, r))}
        onCustomName={(i, name) => run(generation.customName(i, name))}
        onAct={run}
        onBack={() => run(generation.back())}
        onNext={async () => {
          const next = await run(generation.advance());
          if (next && next.view.cursor >= next.view.stages.length) setFinished(true);
        }}
      />
    );
  } else {
    body = (
      <OneTableFlow
        view={view}
        error={response?.error ?? null}
        busy={busy}
        onRoll={(i) => run(generation.roll(i))}
        onChoose={(i, r) => run(generation.choose(i, r))}
        onCustomName={(i, name) => run(generation.customName(i, name))}
        onAct={run}
        onRollRemaining={() => run(generation.rollRemaining())}
        onFinish={() => setFinished(true)}
      />
    );
  }

  return (
    <div className="screen gen-screen" onKeyDown={onKeyDown}>
      <header className="screen-header">
        <div className="screen-header-inner gen-header">
          <button
            type="button"
            className="back-button"
            onClick={() => (anythingResolved ? setConfirmLeave(true) : void leave())}
          >
            <span aria-hidden="true">‹</span> Main Menu
          </button>
          <div className="gen-heading">
            <h1 className="display screen-title">
              {finished && view?.complete ? "Settlement" : "New Settlement"}
            </h1>
            {view && <ModeLabel view={view} />}
          </div>
          {confirmLeave && (
            <ConfirmBar
              message="Leave and discard this settlement? It has not been saved."
              confirmLabel="Discard and leave"
              onConfirm={leave}
              onCancel={() => setConfirmLeave(false)}
            />
          )}
        </div>
      </header>
      <main className="screen-body gen-body">{body}</main>
      <KeyHints />
    </div>
  );
}

function ModeLabel({ view }: { view: SessionView }) {
  const mode = {
    automatic: "Automatic",
    incremental: "Manual · Incremental",
    one_table: "Manual · One-table",
  }[view.mode];
  return (
    <p className="screen-subtitle">
      {mode}
      {view.god_mode && <span className="god-mode-flag"> · God Mode</span>}
    </p>
  );
}

function FatalError({ message, onBack }: { message: string; onBack: () => void }) {
  return (
    <div className="alert" role="alert">
      <p className="alert-title">Generation could not start</p>
      <pre className="alert-detail">{message}</pre>
      <button type="button" className="btn" onClick={onBack}>
        Back to Main Menu
      </button>
    </div>
  );
}

function ErrorNote({ error }: { error: string | null }) {
  if (!error) return null;
  return (
    <p className="alert alert-inline" role="alert">
      {error}
    </p>
  );
}

// ----- Automatic -----------------------------------------------------------

/**
 * Automatic stops only where it needs the user: a choose-only stage (the
 * Setting Profile) or content that is incomplete and must be chosen.
 */
function AutomaticFlow({
  view,
  error,
  busy,
  onChoose,
  onLeave,
}: {
  view: SessionView;
  error: string | null;
  busy: boolean;
  onChoose: (stage: number, result: string) => void;
  onLeave: () => void;
}) {
  const index = view.next_pending;
  if (index === undefined) return <p className="muted gen-loading">Generating…</p>;
  const stage = view.stages[index];
  const blocked = stage.incomplete !== undefined;
  const choosable = stage.options.some((o) => o.legal);

  return (
    <div className="gen-layout">
      <StageRail stages={view.stages} current={index} />
      <section className="gen-main" aria-labelledby="auto-stage-title">
        <span className="label">
          Stage {index + 1} of {view.stages.length}
        </span>
        <h2 id="auto-stage-title" className="display stage-title">
          {blocked ? `${stage.stage.name} needs a choice` : `Choose a ${stage.stage.name}`}
        </h2>
        {blocked ? (
          <p className="alert alert-inline" role="alert">
            {error ?? stage.incomplete}
            {choosable && " Choose a result to continue."}
          </p>
        ) : (
          <p className="muted">Everything after this is rolled automatically.</p>
        )}
        {choosable ? (
          <ResultList
            stage={stage}
            disabled={busy}
            onChoose={(result) => onChoose(index, result)}
          />
        ) : (
          <button type="button" className="btn" onClick={onLeave}>
            Back to Main Menu
          </button>
        )}
      </section>
      <ActiveTags tags={view.active_tags} />
    </div>
  );
}

// ----- Manual: Incremental ---------------------------------------------------

function StagePanel({
  stage,
  total,
  busy,
  onRoll,
  onChoose,
  onCustomName,
  onAct,
}: {
  stage: StageView;
  total: number;
  busy: boolean;
  onRoll: () => void;
  onChoose: (result: string) => void;
  onCustomName: (name: string) => void;
  onAct: (call: Promise<GenerationResponse>) => void;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => heading.current?.focus(), [stage.index]);
  return (
    <section className="gen-main" aria-labelledby={`stage-${stage.index}-title`}>
      <span className="label">
        Stage {stage.index + 1} of {total}
      </span>
      <h2
        ref={heading}
        tabIndex={-1}
        id={`stage-${stage.index}-title`}
        className="display stage-title"
      >
        {stage.stage.name}
      </h2>
      {stage.roles ? (
        <RolesControls stage={stage} busy={busy} onRoll={onRoll} onAct={onAct} />
      ) : stage.figures ? (
        <FiguresControls stage={stage} busy={busy} onRoll={onRoll} onAct={onAct} />
      ) : stage.naming ? (
        <NameControls
          stage={stage}
          busy={busy}
          onRoll={onRoll}
          onChoose={onChoose}
          onCustom={onCustomName}
        />
      ) : (
        <TableStageControls stage={stage} busy={busy} onRoll={onRoll} onChoose={onChoose} />
      )}
    </section>
  );
}

function TableStageControls({
  stage,
  busy,
  onRoll,
  onChoose,
}: {
  stage: StageView;
  busy: boolean;
  onRoll: () => void;
  onChoose: (result: string) => void;
}) {
  const redirected = stage.table.name !== stage.stage.name;
  return (
    <>
      {redirected && <p className="muted stage-table">{stage.table.name}</p>}
      {stage.previous && stage.status === "pending" && (
        <p className="note">
          An earlier change affects this stage. It was <s>{stage.previous.name}</s>.
        </p>
      )}
      {stage.incomplete && <p className="note">{stage.incomplete} Choose a result to continue.</p>}
      {stage.skipped && <p className="note">This stage does not apply and is skipped.</p>}

      {stage.can_roll && (
        <div className="roll-row">
          <button type="button" className="btn btn-primary" disabled={busy} onClick={onRoll}>
            {stage.status === "resolved" ? "Roll again" : "Roll"}
            {stage.dice && " "}
            {stage.dice && <span className="btn-detail">{stage.dice}</span>}
          </button>
          <OutcomeLine outcome={stage.outcome} />
        </div>
      )}
      {!stage.can_roll && <OutcomeLine outcome={stage.outcome} />}

      <span className="label results-label">
        {stage.can_roll ? "Or choose a result" : "Choose a result"}
      </span>
      <ResultList stage={stage} disabled={busy} onChoose={onChoose} />
      <TagList tags={stage.tags} label="Emits" />
    </>
  );
}

function IncrementalFlow({
  view,
  error,
  busy,
  onRoll,
  onChoose,
  onCustomName,
  onAct,
  onBack,
  onNext,
}: {
  view: SessionView;
  error: string | null;
  busy: boolean;
  onRoll: (stage: number) => void;
  onChoose: (stage: number, result: string) => void;
  onCustomName: (stage: number, name: string) => void;
  onAct: (call: Promise<GenerationResponse>) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const index = Math.min(view.cursor, view.stages.length - 1);
  const stage = view.stages[index];
  const last = index === view.stages.length - 1;
  return (
    <div className="gen-layout">
      <StageRail stages={view.stages} current={index} />
      <div>
        <StagePanel
          stage={stage}
          total={view.stages.length}
          busy={busy}
          onRoll={() => onRoll(index)}
          onChoose={(r) => onChoose(index, r)}
          onCustomName={(name) => onCustomName(index, name)}
          onAct={onAct}
        />
        <ErrorNote error={error} />
        <nav className="step-nav" aria-label="Stage navigation">
          <button type="button" className="btn" disabled={busy || index === 0} onClick={onBack}>
            ‹ Previous stage
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || stage.status === "pending"}
            onClick={onNext}
          >
            {last ? "Finish" : "Next stage ›"}
          </button>
        </nav>
      </div>
      <ActiveTags tags={view.active_tags} />
    </div>
  );
}

// ----- Manual: One-table ------------------------------------------------------

function OneTableFlow({
  view,
  error,
  busy,
  onRoll,
  onChoose,
  onCustomName,
  onAct,
  onRollRemaining,
  onFinish,
}: {
  view: SessionView;
  error: string | null;
  busy: boolean;
  onRoll: (stage: number) => void;
  onChoose: (stage: number, result: string) => void;
  onCustomName: (stage: number, name: string) => void;
  onAct: (call: Promise<GenerationResponse>) => void;
  onRollRemaining: () => void;
  onFinish: () => void;
}) {
  return (
    <div className="gen-layout gen-layout-wide">
      <div className="one-table">
        <div className="one-table-toolbar">
          <button
            type="button"
            className="btn"
            disabled={busy || view.complete}
            onClick={onRollRemaining}
          >
            Roll remaining stages
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || !view.complete}
            onClick={onFinish}
          >
            Finish
          </button>
        </div>
        <ErrorNote error={error} />
        <StageGrid
          stages={view.stages}
          busy={busy}
          onRoll={onRoll}
          onChoose={onChoose}
          onCustomName={onCustomName}
          onAct={onAct}
        />
      </div>
      <ActiveTags tags={view.active_tags} />
    </div>
  );
}
