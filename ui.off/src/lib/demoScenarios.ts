import type { DecideRequest } from "./api";

export type DemoScenarioId = "bullish" | "bearish" | "mixed";

export type DemoScenario = {
  id: DemoScenarioId;
  label: string;
  inputs: NonNullable<DecideRequest["inputs"]>;
};

export const DEMO_SCENARIOS: DemoScenario[] = [
  {
    id: "bullish",
    label: "Bullish: cooling inflation + strong jobs",
    inputs: [
      {
        // "Dow Jones futures" anchors hard×3 + semi×2 + verb; "strong gains" scores +4 on top of rally +2
        source: "Reuters",
        text: "Dow Jones futures rally on soft US CPI print; strong gains expected across markets."
      },
      {
        // "Dow industrials" anchors hard×2 + verb (rally); treasury yields = macro
        source: "Bloomberg",
        text: "Dow industrials rally as treasury yields ease after softer core inflation data."
      },
      {
        // BLS weight 0.60 won't trigger, but anchoring Dow Jones keeps this item relevant for history
        source: "BLS",
        text: "Nonfarm payrolls beat forecasts; Dow Jones equities rally on the stronger jobs data."
      },
      {
        // WSJ weight 0.90; "surge" (+3) gives strong positive signal; "Dow Jones stocks" anchors hard×3
        source: "WSJ",
        text: "Dow Jones stocks surge as US and major trading partners agree to a 90-day tariff pause."
      }
    ]
  },
  {
    id: "bearish",
    label: "Bearish: tariff shock + weak macro",
    inputs: [
      {
        // Reuters 0.85; "plunge" (-3) is a clean single-word negative signal; no positive words to cancel
        source: "Reuters",
        text: "White House confirms broad tariff hike on key imports; Dow Jones futures plunge."
      },
      {
        // Changed from ISM (weight 0.60 → no trigger) to Bloomberg (0.85); "slump" (-2) + "contraction" (-2) = -4
        source: "Bloomberg",
        text: "Dow Jones industrials slump as ISM manufacturing PMI falls deeper into contraction."
      },
      {
        // BLS 0.60 won't trigger, but item passes gate for history context
        source: "BLS",
        text: "Payroll data is a miss; Dow equities slump on weak labor market."
      },
      {
        // Changed source from "Fed speech" (default 0.60) to "Fed" (0.95); "warns" (-2) + "plunge" (-3) = -5
        source: "Fed",
        text: "Fed official warns Dow Jones faces a sharp plunge as inflation progress stalls and rates stay higher."
      }
    ]
  },
  {
    id: "mixed",
    label: "Mixed: better CPI but policy uncertainty",
    inputs: [
      {
        // Reuters 0.85 positive trigger: rally (+2) + strong (+2) + gains (+2) = +6 → BUY signal
        source: "Reuters",
        text: "Dow Jones futures rally as headline CPI cools more than expected; broader markets post strong gains."
      },
      {
        // Bloomberg 0.85 negative trigger: slump (-2) + warning (-2) = -4 → SELL signal; conflicts with Reuters → HOLD
        source: "Bloomberg",
        text: "Dow Jones futures slump as Fed pushes back on rate path, warning of persistent inflation."
      },
      {
        // BLS 0.60 won't trigger; "Dow markets" + "Fed" anchors pass relevance gate for history
        source: "BLS",
        text: "Jobs growth solid but Dow markets cautious as Fed weighs wage growth acceleration."
      },
      {
        // Changed source from "Fed minutes" to "Fed" (0.95); neutral sentiment → no trigger; mixed signal context
        source: "Fed",
        text: "Fed minutes reveal split on rate path; Dow Jones volatility expected as inflation risks remain."
      }
    ]
  }
];

export const DEFAULT_DEMO_SCENARIO_ID: DemoScenarioId = "bullish";
