import { translate as t } from "../../../i18n";
import { useHouseEditor } from "./editor/useHouseEditor";
import { InventoryPanel } from "./editor/InventoryPanel";
import { WorkbenchTools } from "./editor/WorkbenchTools";
import { RoomInteraction } from "./editor/RoomInteraction";
import { CursorControls } from "./editor/CursorControls";
import { HouseSurfaces } from "./editor/HouseSurfaces";
import { HouseConfirmation } from "./editor/HouseConfirmation";

export function HouseEditor({ target, initial, art, house, onExit }: Parameters<typeof useHouseEditor>[0]) {
  const model = useHouseEditor({ target, initial, art, house, onExit });
  const {
    reapply,
    undo,
    turn,
    pickup,
    setHand,
    setMoving,
    setSelected,
    setConfirm,
    setWall,
    wall,
    cursorStore,
  } = model;
  return (
    /* eslint-disable-next-line jsx-a11y/no-noninteractive-element-interactions -- Editor shortcuts work while focus is inside this section. */
    <section
      className="house-editor"
      aria-label={t("House editor")}
      onKeyDown={(e) => {
        if ((e.target as HTMLElement).matches("input, select, textarea")) return;
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
          e.preventDefault();
          if (e.shiftKey) reapply();
          else undo();
        } else if (e.key.toLowerCase() === "r") {
          e.preventDefault();
          turn();
        } else if (e.key === "Delete" || e.key === "Backspace") {
          e.preventDefault();
          pickup();
        } else if (e.key === "Escape") {
          setHand(null);
          setMoving(null);
          setSelected(null);
          setConfirm(null);
        } else if (e.key.toLowerCase() === "w")
          setWall(wall === "floor" ? "right" : wall === "right" ? "left" : "floor");
        else if (e.key.startsWith("Arrow")) {
          e.preventDefault();
          const [x, y] = cursorStore.get();
          const next: [number, number] = [
            Math.max(0, x + (e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0)),
            Math.max(0, y + (e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0)),
          ];
          cursorStore.move(next[0], next[1]);
        }
      }}
    >
      <InventoryPanel
        model={{
          search: model.search,
          setSearch: model.setSearch,
          category: model.category,
          setCategory: model.setCategory,
          rarity: model.rarity,
          setRarity: model.setRarity,
          categories: model.categories,
          rarities: model.rarities,
          categoryName: model.categoryName,
          rarityName: model.rarityName,
          inventory: model.inventory,
          stock: model.stock,
          art: model.art,
          hand: model.hand,
          choose: model.choose,
        }}
      />
      <div className="house-workbench">
        <WorkbenchTools
          model={{
            cursorStore: model.cursorStore,
            art: model.art,
            room: model.room,
            others: model.others,
            held: model.held,
            flipped: model.flipped,
            place: model.place,
            turn: model.turn,
            pickup: model.pickup,
            undo: model.undo,
            pending: model.pending,
            selected: model.selected,
            reapply: model.reapply,
            redo: model.redo,
            setConfirm: model.setConfirm,
            onExit: model.onExit,
          }}
        />
        <RoomInteraction
          model={{
            drawn: model.drawn,
            cursorStore: model.cursorStore,
            art: model.art,
            room: model.room,
            others: model.others,
            held: model.held,
            flipped: model.flipped,
            activeWall: model.activeWall,
            ox: model.ox,
            oy: model.oy,
            point: model.point,
            scene: model.scene,
            gesture: model.gesture,
            hand: model.hand,
            setSelected: model.setSelected,
            setFlipped: model.setFlipped,
            target: model.target,
            setMoving: model.setMoving,
            place: model.place,
            send: model.send,
          }}
        />
        <CursorControls
          model={{
            cursorStore: model.cursorStore,
            art: model.art,
            room: model.room,
            others: model.others,
            held: model.held,
            flipped: model.flipped,
            message: model.message,
            activeWall: model.activeWall,
            setWall: model.setWall,
            selected: model.selected,
            setSelected: model.setSelected,
            setHand: model.setHand,
            setMoving: model.setMoving,
            target: model.target,
            pending: model.pending,
            setFlipped: model.setFlipped,
          }}
        />
        <HouseSurfaces
          model={{
            target: model.target,
            room: model.room,
            pending: model.pending,
            surfaces: model.surfaces,
            send: model.send,
            setConfirm: model.setConfirm,
            surfaceName: model.surfaceName,
            house: model.house,
            notesOpen: model.notesOpen,
            setNotesOpen: model.setNotesOpen,
            notes: model.notes,
          }}
        />
        <HouseConfirmation
          model={{
            confirm: model.confirm,
            pending: model.pending,
            send: model.send,
            setConfirm: model.setConfirm,
          }}
        />
      </div>
    </section>
  );
}
