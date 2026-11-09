import type { ReactNode } from "react";
import { Navigate, useLocation } from "react-router";

import type { Me } from "../lib/api";
import { needsLogin } from "../lib/api";
import { useMe } from "../lib/queries";
import { Loading, Problem } from "./States";

/** Renders `children` with the logged-in user, or sends the visitor to the login page. */
export function RequireLogin({ children }: { children: (me: Me) => ReactNode }) {
  const me = useMe();
  const location = useLocation();
  if (me.isPending) return <Loading full />;
  if (me.isError) {
    if (needsLogin(me.error)) {
      const next = encodeURIComponent(location.pathname + location.search);
      return <Navigate to={`/login?next=${next}`} replace />;
    }
    return <Problem full error={me.error} />;
  }
  return <>{children(me.data)}</>;
}
