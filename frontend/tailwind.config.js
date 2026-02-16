/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/**/*.{html,ts}",
    "./src/**/*.component.html",
    "./src/**/*.component.ts"
  ],
  prefix: 'tw-',
  theme: {
    extend: {
      colors: {
        'brand-brown': '#523325',
        'brand-amber': '#F0B429',
        'brand-cream': '#F5F5DC',
        background: 'hsl(var(--color-background))',
        foreground: 'hsl(var(--color-foreground))',
        muted: 'hsl(var(--color-muted))',
        'muted-foreground': 'hsl(var(--color-muted-foreground))',
        border: 'hsl(var(--color-border))',
        accent: 'hsl(var(--color-accent))',
        primary: 'hsl(var(--color-primary))',
      },
      borderRadius: {
        DEFAULT: 'var(--radius)',
      },
      boxShadow: {
        soft: 'var(--shadow-soft)',
        medium: 'var(--shadow-medium)',
        large: 'var(--shadow-large)',
      },
    },
  },
  plugins: [],
}