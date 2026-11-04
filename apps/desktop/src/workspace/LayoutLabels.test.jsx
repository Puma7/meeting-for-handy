import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { OverviewCard } from "./shell/OverviewCard.jsx";
import { WorkspaceLanguage } from "./shell/workspaceLocale.jsx";
it.each(["de", "en"])(
  "separates the insertion label from its module name in %s",
  (locale) => {
    render(
      <WorkspaceLanguage.Provider value={locale}>
        <OverviewCard
          card={{ id: "summary", height: 240 }}
          moving="impulses"
          target={{ anchorId: "summary", edge: "before" }}
        >
          <p>Synthetic card</p>
        </OverviewCard>
      </WorkspaceLanguage.Provider>,
    );
    expect(
      screen.getByText(
        locale === "de"
          ? "Hier einfügen: Gesprächsimpulse"
          : "Insert here: Conversation prompts",
      ),
    ).toBeTruthy();
  },
);
