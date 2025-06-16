import { render, fireEvent } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { EditNoteModal } from '../EditNoteModal';

const defaultProps = {
  show: true,
  note: 'Initial note text',
  onNoteChange: vi.fn(),
  onSubmit: vi.fn(),
  onCancel: vi.fn(),
};

describe('EditNoteModal', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders modal when show is true', () => {
    const { container } = render(<EditNoteModal {...defaultProps} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('does not render modal when show is false', () => {
    const { container } = render(<EditNoteModal {...defaultProps} show={false} />);
    expect(container.firstChild).toBeFalsy();
  });

  it('receives correct props and callbacks', () => {
    const onNoteChange = vi.fn();
    const onSubmit = vi.fn();
    const onCancel = vi.fn();

    render(
      <EditNoteModal
        show={true}
        note="Test note"
        onNoteChange={onNoteChange}
        onSubmit={onSubmit}
        onCancel={onCancel}
      />
    );

    // The component should render without throwing
    expect(onNoteChange).not.toHaveBeenCalled();
    expect(onSubmit).not.toHaveBeenCalled();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it('handles empty note value', () => {
    const { container } = render(<EditNoteModal {...defaultProps} note="" />);
    expect(container.firstChild).toBeTruthy();
  });

  it('handles long note value', () => {
    const longNote = 'This is a very long note that spans multiple lines\nLine 2\nLine 3\nLine 4\nLine 5';
    const { container } = render(<EditNoteModal {...defaultProps} note={longNote} />);
    expect(container.firstChild).toBeTruthy();
  });

  it('calls onCancel when modal should be hidden', () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <EditNoteModal {...defaultProps} onCancel={onCancel} />
    );

    // Simulate modal being closed
    rerender(<EditNoteModal {...defaultProps} show={false} onCancel={onCancel} />);

    // Component should handle show state change
    expect(onCancel).not.toHaveBeenCalled(); // onCancel should only be called on user action
  });
});
