export type Stock = {
  items: { item_id: string; quantity: number }[];
  placed: { item_id: string; room: number }[];
};
export type Surface = {
  name_tr?: string;
  id: string;
  name: string;
  price: number;
  currency: string;
  owned: boolean;
};
