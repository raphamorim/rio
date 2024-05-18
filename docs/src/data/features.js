// @ts-check

import Translate, { translate } from '@docusaurus/Translate';
import CirclesThreePlus from '@site/static/assets/feature-icons/circles-three-plus.svg';
import Image from '@site/static/assets/feature-icons/image.svg';
import Lightning from '@site/static/assets/feature-icons/lightning.svg';
import Palette from '@site/static/assets/feature-icons/palette.svg';
import FontLigatures from '@site/static/assets/feature-icons/font-ligatures.svg';

/**
 * @satisfies {import('@site/src/components/FeaturesSection/index').FeatureCardProps[]}
 */
const FEATURES = [
  {
    title: translate({
      message: 'Fast and Fast',
      id: 'home.features.fast-and-fast.title',
    }),
    Icon: Lightning,
    description: (
      <Translate id="home.features.fast-and-fast.description">
        The Rio has fast performance, leveraging the latest technologies
        including Rust and advanced rendering architectures.
      </Translate>
    ),
  },
  {
    title: translate({
      message: '24-bit true color',
      id: 'home.features.24-bit-true-color.title',
    }),
    Icon: Palette,
    description: (
      <Translate id="home.features.24-bit-true-color.description">
        Regular terminals are limited to just 256 colors, the Rio supports "true
        color," which means it can display up to 16 million colors.
      </Translate>
    ),
  },
  {
    title: translate({
      message: 'Images in Terminal',
      id: 'home.features.images-in-terminal.title',
    }),
    Icon: Image,
    description: (
      <Translate id="home.features.images-in-terminal.description">
        The Rio can display images within the terminal using iTerm2 and kitty
        image protocols.
      </Translate>
    ),
  },
  {
    title: translate({
      message: 'Cross-platform',
      id: 'home.features.cross-platform.title',
    }),
    Icon: CirclesThreePlus,
    description: (
      <Translate id="home.features.cross-platform.description">
        Rio is a cross-platform app that runs on Windows, macOS, Linux, and
        FreeBSD.
      </Translate>
    ),
  },
  {
    title: translate({
      message: 'Font ligatures',
      id: 'home.features.font-ligatures.title',
    }),
    Icon: FontLigatures,
    description: (
      <Translate id="home.features.font-ligatures.description">
        Rio provide font ligatures support as a way to improve readability of
        common expressions or operators.
      </Translate>
    ),
  },
];

export default FEATURES;
