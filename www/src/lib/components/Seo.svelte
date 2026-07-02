<script lang="ts">
  interface Props {
    title: string;
    description: string;
    path?: string;
    image?: string;
    imageAlt?: string;
  }
  let {
    title,
    description,
    path = '/',
    image = '/img/og/postern-og.jpg',
    imageAlt = 'Postern self-hosted email client interface'
  }: Props = $props();

  const base = 'https://postern.email';
  let url = $derived(base + path);
  let ogImage = $derived(base + image);
  let fullTitle = $derived(path === '/' ? title : `${title} · Postern`);
  const jsonLd = $derived(
    JSON.stringify({
      '@context': 'https://schema.org',
      '@type': 'SoftwareApplication',
      name: 'Postern',
      applicationCategory: 'EmailApplication',
      operatingSystem: 'Linux, Web',
      url,
      description,
      image: ogImage,
      offers: [
        {
          '@type': 'Offer',
          name: 'Postern Community',
          price: '0',
          priceCurrency: 'USD'
        },
        {
          '@type': 'Offer',
          name: 'Postern Pro',
          price: '100',
          priceCurrency: 'GBP',
          url: 'https://billing.postern.email/'
        }
      ],
      softwareHelp: 'https://docs.postern.email/',
      codeRepository: 'https://github.com/dazller4554328/postern-community',
      license: 'https://www.apache.org/licenses/LICENSE-2.0'
    })
  );
</script>

<svelte:head>
  <title>{fullTitle}</title>
  <meta name="description" content={description} />
  <link rel="canonical" href={url} />

  <meta property="og:type" content="website" />
  <meta property="og:site_name" content="Postern" />
  <meta property="og:title" content={fullTitle} />
  <meta property="og:description" content={description} />
  <meta property="og:url" content={url} />
  <meta property="og:image" content={ogImage} />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:image:alt" content={imageAlt} />

  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:title" content={fullTitle} />
  <meta name="twitter:description" content={description} />
  <meta name="twitter:image" content={ogImage} />
  <meta name="twitter:image:alt" content={imageAlt} />
  <!-- {@html} is ignored inside a <script> element, so inject the whole tag
       at head level. Escape `<` so the JSON can't break out of the script. -->
  {@html `<script type="application/ld+json">${jsonLd.replace(/</g, '\\u003c')}</script>`}
</svelte:head>
