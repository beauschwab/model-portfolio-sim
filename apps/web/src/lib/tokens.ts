/** Read an Aperture design token (CSS custom property from styles/aperture.css)
 * for contexts that cannot resolve var() themselves, such as canvas 2D. */
export function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
