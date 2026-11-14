export type Item = {
  id: string;
  name: string;
  quantity: number;
  category: string;
  rarity_name: string;
  category_name: string;
  bound: boolean;
  placed: { room: number; count: number }[];
};
export type Friend = { id: string; name: string; since: string };
