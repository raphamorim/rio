// @ts-check

import MENTIONS from '@site/src/data/mentions';
import Heading from '@theme/Heading';
import clsx from 'clsx';
import styles from './styles.module.css';

/**
 * @typedef {Object} MentionCardProps
 * @property {string} tweetUrl
 * @property {string} username
 * @property {import('react').ReactNode} quote
 *
 * @param {MentionCardProps} props
 */
function MentionCard(props) {
  const { tweetUrl, username, quote } = props;

  return (
    <figure className={styles.mentionCard}>
      <div className={styles.mentionCardTop}>
        <img
          width={40}
          height={40}
          src={`https://unavatar.io/twitter/${username}`}
          decoding="async"
          loading="lazy"
          alt=""
          className={styles.mentionCardAvatar}
        />
        <figcaption>
          <cite>
            <a href={tweetUrl} className={styles.mentionCardUsername}>
              @{username}
            </a>
          </cite>
        </figcaption>
      </div>
      <blockquote cite={tweetUrl} className={styles.mentionCardQuote}>
        {quote}
      </blockquote>
    </figure>
  );
}

/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
const COLUMNS_COUNT = 3;
/** @type {MentionCardProps[][]} */
const COLUMNS = Array.from({ length: COLUMNS_COUNT }, () => []);
MENTIONS.forEach((mention, i) => {
  COLUMNS[i % 3].push(mention);
});

export default function MentionsSection() {
  return (
    <section className={clsx(styles.mentionsSection, 'container')}>
      <Heading as="h2" className={styles.title}>
        Loved by many engineers
      </Heading>
      <div className={clsx('row', styles.mentions)}>
        {COLUMNS.map((column, i) => (
          <div className="col col--4" key={i}>
            {column.map((mention) => (
              <MentionCard key={mention.tweetUrl} {...mention} />
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}
