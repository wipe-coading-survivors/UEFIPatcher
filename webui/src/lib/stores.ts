import { writable } from 'svelte/store';

export const sessionStore = writable<string | null>(null);
export const imageStore = writable<string | null>(null);
export const treeStore = writable<TreeNode[]>([]);
export const selectedStore = writable<TreeNode | null>(null);

export interface TreeNode {
    path: string;
    type: number;
    subtype: number;
    guid: string;
    offset: number;
    size: number;
    name: string;
}
