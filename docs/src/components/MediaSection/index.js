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
        <iframe width="100%" height="500" src="https://www.youtube.com/embed/c47cFF2k8_0?si=3wdEJMYD6HN_K50j" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>
      </div>
    </section>
  );
}
