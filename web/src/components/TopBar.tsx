import { Link } from "react-router";

import type { Me } from "../lib/api";
import { useLogout } from "../lib/queries";
import { Crab } from "./Crab";
import { PixelButton } from "./PixelButton";

export function TopBar({ me }: { me: Me }) {
  const logout = useLogout();
  const name = me.user.global_name ?? me.user.username;
  return (
    <header className="top-bar">
      <Link to="/servers" className="brand" aria-label="Pinchora dashboard home">
        <Crab scale={3} />
        <span className="brand-name">PINCHORA</span>
        <span className="brand-tag">DASHBOARD</span>
      </Link>
      <nav className="top-bar-links" aria-label="Player pages">
        <Link to="/me/house" className="top-bar-link">
          My house
        </Link>
      </nav>
      <div className="top-bar-user">
        <img className="avatar" src={me.user.avatar_url} alt="" width={32} height={32} />
        <span className="top-bar-name">{name}</span>
        <PixelButton variant="ghost" onClick={() => logout.mutate()} disabled={logout.isPending}>
          Log out
        </PixelButton>
      </div>
    </header>
  );
}
