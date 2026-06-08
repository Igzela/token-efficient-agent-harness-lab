"use client";

import { useState } from "react";

export type TabId = string;

export interface TabDef {
  id: TabId;
  label: string;
}

export interface TabGroupDef {
  label: string;
  tabs: TabDef[];
  collapsible?: boolean;
}

export function TabGroup({
  groups,
  activeTab,
  onTabChange,
}: {
  groups: TabGroupDef[];
  activeTab: TabId;
  onTabChange: (id: TabId) => void;
}) {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  function toggle(groupLabel: string) {
    setCollapsed((prev) => ({ ...prev, [groupLabel]: !prev[groupLabel] }));
  }

  return (
    <nav className="nav" aria-label="Dashboard sections" role="tablist">
      {groups.map((group) => {
        const isCollapsed = collapsed[group.label] ?? false;
        const visibleTabs = group.collapsible && isCollapsed
          ? group.tabs.filter((t) => t.id === activeTab)
          : group.tabs;

        return (
          <div className="tab-group" key={group.label}>
            <span className="tab-group-label">{group.label}</span>
            <div className="tab-group-tabs">
              {visibleTabs.map((item) => (
                <button
                  aria-selected={item.id === activeTab}
                  className="tab"
                  key={item.id}
                  onClick={() => onTabChange(item.id)}
                  role="tab"
                  type="button"
                >
                  {item.label}
                </button>
              ))}
              {group.collapsible && (
                <button
                  className="tab tab-toggle"
                  onClick={() => toggle(group.label)}
                  type="button"
                  aria-label={isCollapsed ? `Show ${group.label} tabs` : `Hide ${group.label} tabs`}
                >
                  {isCollapsed ? "More" : "Less"}
                </button>
              )}
            </div>
          </div>
        );
      })}
    </nav>
  );
}
