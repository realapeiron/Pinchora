import sprite from "./sprite.svg?raw";

export function IconSprite() {
  return <span className="icon-sprite" aria-hidden="true" dangerouslySetInnerHTML={{ __html: sprite }} />;
}
