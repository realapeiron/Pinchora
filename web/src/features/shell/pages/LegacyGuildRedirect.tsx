import { Navigate, useLocation, useParams } from "react-router";

export function legacyGuildTarget(guildId: string, target: string, search: string, hash: string) {
  return `/g/${guildId}/${target}${search}${hash}`;
}

export function LegacyGuildRedirect({ target }: { target: string }) {
  const { guildId } = useParams();
  const location = useLocation();
  return <Navigate to={legacyGuildTarget(guildId ?? "", target, location.search, location.hash)} replace />;
}
