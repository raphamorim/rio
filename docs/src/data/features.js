// @ts-check

import CirclesThreePlus from '@site/static/assets/feature-icons/circles-three-plus.svg';
import Image from '@site/static/assets/feature-icons/image.svg';
import Lightning from '@site/static/assets/feature-icons/lightning.svg';
import Palette from '@site/static/assets/feature-icons/palette.svg';

/**
 * @satisfies {import('@site/src/components/FeaturesSection/index').FeatureCardProps[]}
 */
const FEATURES = [
  {
    title: 'Fast and Fast',
    Icon: Lightning,
    description: (
      <>
        The Rio has fast performance, leveraging the latest technologies
        including Rust and advanced rendering architectures.
      </>
    ),
  },
  {
    title: '24-bit true color',
    Icon: Palette,
    description: (
      <>
        Regular terminals are limited to just 256 colors, the Rio supports "true
        color," which means it can display up to 16 million colors.
      </>
    ),
  },
  {
    title: 'Images in Terminal',
    Icon: Image,
    description: (
      <>
        The Rio can display images within the terminal using iTerm2 and kitty
        image protocols.
      </>
    ),
  },
  {
    title: 'Cross-platform',
    Icon: CirclesThreePlus,
    description: (
      <>
        Rio is a cross-platform app that runs on Windows, macOS, Linux, and
        FreeBSD.
      </>
    ),
  },
];

export default FEATURES;
