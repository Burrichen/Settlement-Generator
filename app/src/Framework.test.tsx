// Flow stages and modifier presets, rendered from the structures the core
// sends (no IPC involved).
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { EffectDef, EffectKindSchema, StageView } from "./api";
import { EffectEditor } from "./components/editors/EffectEditor";
import { FlowControls, FlowOutline } from "./components/FlowRecord";

afterEach(cleanup);

function flowStage(patch: Partial<StageView> = {}): StageView {
  return {
    index: 3,
    stage: { id: "step.omens", name: "Omens" },
    table: { id: "step.omens", name: "Omens" },
    status: "resolved",
    current: false,
    actionable: true,
    can_roll: true,
    options: [],
    result: { id: "omens", name: "2 × Omen" },
    flow: {
      outline: [
        { depth: 0, label: "Omen 1", value: "Omen" },
        { depth: 1, label: "Colour", value: "Red" },
        { depth: 0, label: "Omen 2", value: "Omen" },
        { depth: 1, label: "Colour", value: "Blue" },
      ],
    },
    ...patch,
  };
}

describe("flow stages", () => {
  it("shows each line of the record, indented by depth", () => {
    render(<FlowOutline lines={flowStage().flow?.outline} />);
    const items = screen.getAllByRole("listitem");
    expect(items.map((i) => i.textContent)).toEqual([
      "Omen 1: Omen",
      "Colour: Red",
      "Omen 2: Omen",
      "Colour: Blue",
    ]);
    expect(items[1].style.paddingInlineStart).toBe("1.25rem");
  });

  it("rolls the whole stage again", async () => {
    const user = userEvent.setup();
    const onRoll = vi.fn();
    render(<FlowControls stage={flowStage()} busy={false} onRoll={onRoll} />);
    await user.click(screen.getByRole("button", { name: "Roll again" }));
    expect(onRoll).toHaveBeenCalledOnce();
  });

  it("says when the stage does not apply and offers no roll", () => {
    render(
      <FlowControls
        stage={flowStage({ status: "skipped", skipped: "not_eligible", flow: {} })}
        busy={false}
        onRoll={() => {}}
      />,
    );
    expect(screen.getByText("This stage does not apply and is skipped.")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });
});

describe("modifier presets", () => {
  const shift: EffectKindSchema = {
    kind: "shift",
    label: "Shift",
    description: "Moves the result along the scale.",
    executable: true,
    group: "Change the roll",
    fields: [
      {
        name: "steps",
        label: "Steps",
        kind: { type: "integer" },
        required: true,
        help: "",
      },
    ],
    presets: [
      { label: "Shift +1", values: { steps: 1 } },
      { label: "Shift -2", values: { steps: -2 } },
    ],
  };

  it("fill in the effect's fields in one click", async () => {
    const user = userEvent.setup();
    let effect: EffectDef = { kind: "shift", table: "table.size" };
    const onChange = vi.fn((next: EffectDef) => (effect = next));
    render(
      <EffectEditor
        effect={effect}
        schema={[shift]}
        label="Effect 1"
        onChange={onChange}
        onRemove={() => {}}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Shift -2" }));
    expect(effect).toEqual({ kind: "shift", table: "table.size", steps: -2 });
  });
});
