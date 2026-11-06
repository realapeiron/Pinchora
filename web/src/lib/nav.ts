// The sidebar's page list, and what the filter box keeps of it.

export interface NavPage {
  /** Path under `/g/:guildId`; empty for the overview. */
  to: string;
  label: string;
}

export interface NavGroup {
  group: string;
  pages: NavPage[];
}

/**
 * The groups and pages to show for what was typed in the filter box: a page whose name contains it
 * (or any page of a group whose name does), in any case. A group left with no page is dropped, and
 * so is a group no feature has added a page to yet.
 */
export function visibleNav(nav: NavGroup[], query: string): NavGroup[] {
  const wanted = query.trim().toLowerCase();
  return nav.flatMap(({ group, pages }) => {
    const kept =
      wanted === "" || group.toLowerCase().includes(wanted)
        ? pages
        : pages.filter((page) => page.label.toLowerCase().includes(wanted));
    return kept.length > 0 ? [{ group, pages: kept }] : [];
  });
}
