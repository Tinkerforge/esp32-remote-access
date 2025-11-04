import { render, screen, fireEvent } from '@testing-library/preact';
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
    render(<EditNoteModal {...defaultProps} />);
    expect(screen.getByText('edit_note_heading')).toBeInTheDocument();
  });

  it('does not render modal when show is false', () => {
    render(<EditNoteModal {...defaultProps} show={false} />);
    expect(screen.queryByText('edit_note_heading')).not.toBeInTheDocument();
  });

  it('displays the note text in textarea', () => {
    render(<EditNoteModal {...defaultProps} />);
    expect(screen.getByDisplayValue('Initial note text')).toBeInTheDocument();
  });

  it('displays accept and decline buttons', () => {
    render(<EditNoteModal {...defaultProps} />);
    expect(screen.getByText('accept')).toBeInTheDocument();
    expect(screen.getByText('decline')).toBeInTheDocument();
  });

  it('calls onNoteChange when textarea value changes', () => {
    render(<EditNoteModal {...defaultProps} />);
    const textarea = screen.getByDisplayValue('Initial note text');

    fireEvent.change(textarea, { target: { value: 'Updated note text' } });

    expect(defaultProps.onNoteChange).toHaveBeenCalledWith('Updated note text');
  });

  it('calls onSubmit when form is submitted', () => {
    render(<EditNoteModal {...defaultProps} />);
    const form = screen.getByText('accept').closest('form');
    expect(form).not.toBeNull();

    fireEvent.submit(form as HTMLFormElement);

    expect(defaultProps.onSubmit).toHaveBeenCalled();
  });

  it('calls onCancel when decline button is clicked', () => {
    render(<EditNoteModal {...defaultProps} />);
    const declineButton = screen.getByText('decline');

    fireEvent.click(declineButton);

    expect(defaultProps.onCancel).toHaveBeenCalled();
  });

  it('handles empty note value', () => {
    render(<EditNoteModal {...defaultProps} note="" />);
    const textarea = screen.getByRole('textbox');
    expect(textarea).toHaveValue('');
  });

  it('handles long note value', () => {
    const longNote = 'This is a very long note that spans multiple lines\nLine 2\nLine 3\nLine 4\nLine 5';
    render(<EditNoteModal {...defaultProps} note={longNote} />);
    const textarea = screen.getByRole('textbox');
    expect(textarea).toBeInTheDocument();
  });

  it('renders textarea as an editable field', () => {
    render(<EditNoteModal {...defaultProps} />);
    const textarea = screen.getByRole('textbox');
    expect(textarea).not.toBeDisabled();
  });
});
