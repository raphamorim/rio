// @ts-check

import FEATURES from '@site/src/data/features';
import Heading from '@theme/Heading';
import clsx from 'clsx';
import styles from './styles.module.css';

/**
 * @typedef {import('react').ComponentPropsWithoutRef<'svg'>} SvgProps
 *
 * @typedef {Object} FeatureCardProps
 * @property {import('react').ComponentType<SvgProps>} Icon
 * @property {string} title
 * @property {import('react').ReactNode} description
 *
 * @param {FeatureCardProps} props
 */
function FeatureCard(props) {
  const { Icon, title, description } = props;

  return (
    <div className={styles.featureCard}>
      <div className={styles.iconWrapper}>
        <Icon />
      </div>
      <div className={styles.textSection}>
        <Heading as="h2" className={styles.title}>
          {title}
        </Heading>
        <p className={styles.description}>{description}</p>
      </div>
    </div>
  );
}

export default function FeaturesSection() {
  return (
    <section className={clsx(styles.features, 'container')}>
      {FEATURES.map((props, idx) => (
        <FeatureCard key={idx} {...props} />
      ))}
    </section>
  );
}
