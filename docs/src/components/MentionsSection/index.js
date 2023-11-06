/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// @ts-check

import Link from '@docusaurus/Link';
import MENTIONS from '@site/src/data/mentions';
import Heading from '@theme/Heading';
import clsx from 'clsx';
import styles from './styles.module.css';

function Mention({ url, handle, name, content, date, githubUsername }) {
  return (
    <div className={clsx('card', styles.mention)}>
      <div className="card__header">
        <div className="avatar">
          <img
            alt={name}
            className="avatar__photo"
            src={`https://unavatar.io/twitter/${handle}?fallback=https://github.com/${githubUsername}.png`}
            width="48"
            height="48"
            loading="lazy"
          />
          <div className={clsx('avatar__intro', styles.mentionMeta)}>
            <strong className="avatar__name">{name}</strong>
            <span>@{handle}</span>
          </div>
        </div>
      </div>

      <div className={clsx('card__body', styles.mention)}>{content}</div>

      <div className="card__footer">
        <Link className={clsx(styles.mentionMeta, styles.mentionDate)} to={url}>
          {date}
        </Link>
      </div>
    </div>
  );
}

export default function MentionsSection() {
  let columns = [[], [], []];
  MENTIONS.filter((mention) => mention.showOnHomepage).forEach((mention, i) =>
    columns[i % 3].push(mention),
  );

  return (
    <div className={clsx(styles.section, styles.sectionAlt)}>
      <div className="container">
        <Heading as="h2" className={clsx('margin-bottom--lg', 'text--center')}>
          Loved by many engineers
        </Heading>
        <div className={clsx('row', styles.mentionsSection)}>
          {columns.map((items, i) => (
            <div className="col col--4" key={i}>
              {items.map((tweet) => (
                <Mention {...tweet} key={tweet.url} />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
