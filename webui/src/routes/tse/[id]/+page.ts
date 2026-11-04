import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => ({ imageId: params.id });
