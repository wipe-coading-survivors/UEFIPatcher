export function sectionHref(id: string, imageId: string | null | undefined): string {
    if (id === 'artifacts' || !imageId) return `/${id}`;
    return `/${id}/${imageId}`;
}
