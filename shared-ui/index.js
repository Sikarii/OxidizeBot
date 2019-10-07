import React from "react";
import ReactMarkdown from 'react-markdown';

function Header(props) {
  return <ReactMarkdown source={props.source} />
}

function Content(props) {
  return <ReactMarkdown source={props.source} />
}

function ExampleContent(props) {
  return <pre><code>{props.source}</code></pre>
}

class Example extends React.Component {
  constructor(props) {
    super(props);
  }

  render() {
    return <>
      <div className="example-name"><b>Example:</b> <Header source={this.props.name} /></div>
      <div className="example-content"><ExampleContent source={this.props.content} /></div>
    </>;
  }
}

class Command extends React.Component {
  constructor(props) {
    super(props);
  }

  render() {
    let examples = null;

    if (this.props.examples && this.props.examples.length > 0) {
      examples = <div className="command-examples">
        {(this.props.examples || []).map((e, i) => {
          return <Example key={i} {...e} />;
        })}
      </div>;
    }

    return <>
      <tr className="command">
        <td>
          <div className="command-name"><Header source={this.props.name} /></div>
          <div className="command-content"><Content source={this.props.content} /></div>

          {examples}
        </td>
      </tr>
    </>;
  }
}

export class CommandGroup extends React.Component {
  constructor(props) {
    super(props);

    this.state = {
      expanded: false,
    };
  }

  toggle(expanded) {
    this.setState({expanded});
  }

  render() {
    let commands = null;

    let expand = this.state.expanded || !this.props.expandable || !!this.props.modified;

    if (this.props.commands && this.props.commands.length > 0 && expand) {
      commands = <table className="table table-dark table-striped">
        <tbody>
          {(this.props.commands || []).map((c, i) => {
            return <Command key={i} {...c} />;
          })}
        </tbody>
      </table>;
    }

    let show = null;

    if (this.props.commands.length > 0 && !this.props.modified && this.props.expandable) {
      if (!this.state.expanded) {
        show = <button className="btn btn-info btn-sm" onClick={() => this.toggle(true)}>
          Show
        </button>;
      } else {
        show = <button className="btn btn-info btn-sm" onClick={() => this.toggle(false)}>
          Hide
        </button>;
      }
    }

    return <>
      <div className="command-group">
        <div className="command-group-name">
          {this.props.name}
        </div>

        <div className="command-group-content"><ReactMarkdown source={this.props.content} /></div>

        <div className="command-group-actions">{show}</div>

        {commands}
      </div>
    </>;
  }
}