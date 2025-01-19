// @ts-check

import Translate from '@docusaurus/Translate';
import Heading from '@theme/Heading';
import clsx from 'clsx';
import styles from './styles.module.css';

export default function MentionsSection() {
  return (
    <section className={clsx(styles.mediaSection, 'container')}>
      <Heading as="h2" className={styles.title}>
        <Translate>Latest video update</Translate>
      </Heading>
      <div className={clsx('row', styles.media)}>
        <iframe width="100%" height="500" src="https://www.youtube.com/embed/AXb78boK8K4?si=ZvxYzyPxtxRWxv4i" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>
      </div>
    </section>
  );
}
