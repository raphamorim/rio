// @ts-check

import Translate from '@docusaurus/Translate';
import Heading from '@theme/Heading';
import clsx from 'clsx';
import styles from './styles.module.css';
// <img height="40%" src="assets/demo-pokemon.gif" />

export default function DemoSection() {
  return (
    <section className={clsx(styles.mediaSection, 'container')}>
      <Heading as="h2" className={styles.title}>
        <Translate>Unleash terminal true power</Translate>
      </Heading>
      <div className={clsx('row', styles.media)}>
        <video width="60%" autoplay loop muted playsinline>
          <source src="assets/demo-rio-pokemon.webm" type="video/webm" />
          <source src="assets/demo-rio-pokemon.mp4" type="video/mp4" />
          Your browser does not support the video format.
        </video>
      </div>
    </section>
  );
}
