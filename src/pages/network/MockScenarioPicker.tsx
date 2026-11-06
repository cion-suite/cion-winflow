import { useT } from "@/lib/i18n";
import { ToggleGroup, ToggleGroupItem } from "@/ui/shadcn/toggle-group";
import type { MockScenario } from "./mock";

const SCENARIOS: readonly MockScenario[] = ["proxy", "tun", "vpn-off", "offline"];

const MOCK_KEYS = {
  proxy: "network.mock.states.proxy",
  tun: "network.mock.states.tun",
  "vpn-off": "network.mock.states.vpnOff",
  offline: "network.mock.states.offline",
} as const;

function isScenario(value: string): value is MockScenario {
  return (SCENARIOS as readonly string[]).includes(value);
}

type MockScenarioPickerProps = {
  value: MockScenario;
  onChange: (scenario: MockScenario) => void;
};

/** Временная панель макета: видна только в dev-сборке. */
export function MockScenarioPicker({ value, onChange }: MockScenarioPickerProps) {
  const t = useT();

  return (
    <div className="flex w-fit max-w-full items-center gap-2 rounded-lg border border-dashed px-1 py-0.5">
      <span className="hidden pl-1 text-xs text-muted-foreground sm:inline">
        {t("network.mock.label")}
      </span>
      <ToggleGroup
        aria-label={t("network.mock.label")}
        value={[value]}
        onValueChange={(next) => {
          const [picked] = next;
          if (picked !== undefined && isScenario(picked)) onChange(picked);
        }}
        variant="outline"
        size="sm"
        spacing={0}
      >
        {SCENARIOS.map((scenario) => (
          <ToggleGroupItem
            key={scenario}
            value={scenario}
            className="px-2 text-xs aria-pressed:bg-secondary aria-pressed:text-secondary-foreground"
          >
            {t(MOCK_KEYS[scenario])}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}
